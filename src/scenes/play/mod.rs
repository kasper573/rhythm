mod background;
mod clock;
mod tuning;

use crate::core::assets::{asset_root, asset_server_path};
use crate::core::audio::{Sound, SoundChannel, SoundPlayer};
use crate::core::config::GameConfig;
use crate::core::input::{Actions, GameAction, StepDirection};
use crate::core::library::{StepfileEntry, StepfileId, StepfileLibrary};
use crate::core::platform::SoundOptions;
use crate::core::player::PlayerId;
use crate::core::scene_flow::SpawnScoped;
use crate::core::settings::{GradeLayer, MachineSettings, NoteSpeed, PlayerSettings};
use crate::core::sfx::{PlaySfx, Sfx};
use crate::core::stepfile::{MusicPlayer, StepfileClock};
use crate::core::tick_track::render_tick_track;
use crate::core::units::Seconds;
use crate::core::{OVERLAY_LAYER, SCREEN_SIZE, visible_world_size};
use crate::prefabs::health_vial::{
    HealthVial, HealthVialMaterial, HealthVialPrefabOptions, VialSide, health_vial_prefab,
};
use crate::prefabs::stepfile_player::note_field::{
    NoteField, NoteFieldClock, NoteFieldSystems, TARGET_Y, fitted_arrow_size, max_arrow_size,
};
use crate::prefabs::stepfile_player::note_skin::ActiveNoteSkins;
use crate::prefabs::stepfile_player::{
    FieldSpec, ForPlayer, GameplayDrive, PlayInput, PlaySession, PlaySet, StageFailed,
    StageResults, StepfilePlayerAssets, StepfilePlayerPrefabOptions, clear_session, grade_text,
    stepfile_player_prefab,
};
use crate::scenes::wheel::{SelectedStepfile, WheelTarget};
use crate::scenes::{GameScene, SceneFade, scene_accepts_input};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use strum::IntoEnumIterator;

/// The play scene's outcome, read by the score scene.
#[derive(Resource, Debug, Clone)]
pub struct ScoreResults {
    pub id: StepfileId,
    pub title: String,
    pub players: Vec<PlayerResult>,
}

/// One player's complete run: the chart they played and its results.
#[derive(Debug, Clone)]
pub struct PlayerResult {
    /// Index into the played stepfile's `charts`.
    pub chart: usize,
    pub stage: StageResults,
}

/// The play scene: the real gameplay adapter around the stepfile player.
/// It fills the engine's ports from the audio clock (see `clock`) and the
/// keyboard, composes the stage furniture (health vials, backgrounds, the
/// tuning HUD), and turns the session's end into [`ScoreResults`].
pub(super) struct PlayScenePlugin;

impl Plugin for PlayScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((clock::plugin, tuning::plugin, background::plugin))
            .add_systems(OnEnter(GameScene::Play), enter)
            .add_systems(OnExit(GameScene::Play), exit)
            .add_systems(
                Update,
                (
                    wire_keyboard.in_set(GameplayDrive),
                    (anchor_stage_to_window, sync_health_vials).in_set(PlaySet::Sync),
                    (
                        set_stage_grade_area,
                        react_to_failures,
                        finish_when_complete,
                        handle_cancel.run_if(scene_accepts_input),
                    )
                        .chain()
                        .in_set(PlaySet::Present),
                )
                    .run_if(in_state(GameScene::Play)),
            )
            .add_systems(
                Update,
                refit_stages_to_window
                    .before(NoteFieldSystems)
                    .run_if(in_state(GameScene::Play)),
            );
    }
}

/// The scene's session flow around the engine: the playback clock that
/// drives the ports, the moment the chart is over, and whether the run has
/// concluded.
#[derive(Resource)]
struct Playback {
    title: String,
    phase: PlayPhase,
    /// The shared stepfile music clock, servo'd onto the audio.
    music: StepfileClock,
    /// Wall-clock time since the tracks were started, for measuring how far
    /// the mixer's queue runs ahead of real time (the audio latency).
    wall_since_play: Seconds,
    latency_samples: Vec<Seconds>,
    last_note_time: Seconds,
    finished: bool,
}

enum PlayPhase {
    LeadIn { remaining: Seconds },
    Playing,
}

#[derive(Component, Default, Clone)]
struct MusicTrack;

/// The pre-rendered tick track sink, always playing in sync, muted unless
/// tick audio is toggled on.
#[derive(Component, Default, Clone)]
struct TickTrack;

/// The sources the stage is materialized from, and the window it is
/// fitted to.
#[derive(SystemParam)]
struct StageAssets<'w, 's> {
    skins: Res<'w, ActiveNoteSkins>,
    asset_server: Res<'w, AssetServer>,
    images: ResMut<'w, Assets<Image>>,
    sounds: ResMut<'w, Assets<Sound>>,
    vial_materials: ResMut<'w, Assets<HealthVialMaterial>>,
    machine: Res<'w, MachineSettings>,
    windows: Query<'w, 's, &'static Window>,
}

/// The grade-text source layer per stage player, clear of the lane and
/// overlay layers.
const STAGE_GRADE_SOURCE_BASE: usize = 20;

fn enter(
    mut commands: Commands,
    selected: Option<Res<SelectedStepfile>>,
    library: Res<StepfileLibrary>,
    config: Res<GameConfig>,
    player_settings: Res<PlayerSettings>,
    mut assets: StageAssets,
    mut fade: ResMut<SceneFade>,
) {
    let Some(selected) = selected else {
        fade.begin(GameScene::Wheel);
        return;
    };
    let entry = library.stepfile(selected.id);
    let timing = entry.stepfile.timing.clone();

    let Some(charts) = selected
        .charts
        .iter()
        .map(|player_chart| {
            entry
                .stepfile
                .charts
                .get(player_chart.chart)
                .map(|chart| (player_chart.player, chart))
        })
        .collect::<Option<Vec<_>>>()
    else {
        fade.begin(GameScene::Wheel);
        return;
    };
    if charts.is_empty() {
        fade.begin(GameScene::Wheel);
        return;
    }
    let specs: Vec<PackSpec> = charts
        .iter()
        .map(|(player, chart)| PackSpec {
            player: *player,
            columns: chart.columns,
            speed: player_settings[*player].note_speed,
        })
        .collect();
    let layouts = pack_stage_fields(&specs, &config, assets.windows.single().ok());

    let fields: Vec<FieldSpec> = charts
        .iter()
        .zip(&layouts)
        .map(|((player, chart), layout)| {
            let grade_layer = player_settings[*player].grade_layer;
            FieldSpec {
                layout: layout.clone(),
                rows: &chart.rows,
                mines: &chart.mines,
                grade_source_layer: STAGE_GRADE_SOURCE_BASE + player_index(*player),
                grade_present_layer: (grade_layer == GradeLayer::InFront).then_some(OVERLAY_LAYER),
                popup_layer: Some(OVERLAY_LAYER),
                max_health: config.player_max_health,
            }
        })
        .collect();
    let last_note_time = stepfile_player_prefab(
        StepfilePlayerPrefabOptions {
            fields,
            timing: timing.clone(),
            scope: DespawnOnExit(GameScene::Play),
        },
        &mut commands,
        &mut StepfilePlayerAssets {
            asset_server: &assets.asset_server,
            images: &mut assets.images,
            config: &config,
            skins: &assets.skins,
        },
    );

    for (player, _) in &charts {
        let side = match player {
            PlayerId::P1 => VialSide::Left,
            PlayerId::P2 => VialSide::Right,
        };
        let vial = health_vial_prefab(
            HealthVialPrefabOptions {
                fill: 1.0,
                side,
                edge_padding: config.stage.screen_edge_padding,
            },
            &mut commands,
            &mut assets.vial_materials,
        );
        commands
            .entity(vial)
            .insert((ForPlayer(*player), DespawnOnExit(GameScene::Play)));
    }

    let tick_times: Vec<Seconds> = charts
        .iter()
        .flat_map(|(_, chart)| {
            chart
                .rows
                .iter()
                .map(|row| timing.seconds_at_beat(row.beat))
        })
        .collect();
    spawn_audio_tracks(&mut commands, &mut assets, entry, &tick_times, &config);
    background::spawn_background(&mut commands, entry, &timing);

    let lead_in = Seconds(config.stage.lead_in_seconds);
    commands.insert_resource(NoteFieldClock {
        visible: -lead_in,
        timing: timing.clone(),
        target_y: TARGET_Y,
    });
    commands.insert_resource(Playback {
        title: entry.display_title(),
        phase: PlayPhase::LeadIn { remaining: lead_in },
        music: StepfileClock::start_at(timing, -lead_in),
        wall_since_play: Seconds::ZERO,
        latency_samples: Vec::new(),
        last_note_time,
        finished: false,
    });
}

fn exit(mut commands: Commands, mut music: ResMut<MusicPlayer>) {
    music.stop();
    clear_session(&mut commands);
    commands.remove_resource::<NoteFieldClock>();
    commands.remove_resource::<grade_text::GradeArea>();
    commands.remove_resource::<Playback>();
    commands.remove_resource::<SelectedStepfile>();
}

/// The real adapter's input driver: fills the [`PlayInput`] port from the
/// keyboard. Cleared rather than skipped while the scene fade runs, so a
/// departing stage grants no input.
fn wire_keyboard(actions: Actions, fade: Res<SceneFade>, input: Option<ResMut<PlayInput>>) {
    let Some(mut input) = input else { return };
    input.clear();
    if !fade.accepts_input() {
        return;
    }
    for player in PlayerId::iter() {
        for column in 0..4 {
            let action = GameAction::step(player, StepDirection::of_column(column));
            if actions.pressed(action) {
                input.press(action, actions.just_pressed(action));
            }
        }
    }
}

/// What a stage field is packed from, independent of whether it exists yet.
struct PackSpec {
    player: PlayerId,
    columns: usize,
    speed: NoteSpeed,
}

/// Sizes and places one field per stage: arrows grow to the configured
/// pixel cap (`stage.max_arrow_size`) when the window has room and shrink
/// until every column — plus the gaps between fields — fits between the
/// reserved screen edges. The fields pack left-to-right, centered as a
/// block, across the window's visible world width. Headless callers (no
/// window) pack on the design canvas.
fn pack_stage_fields(
    specs: &[PackSpec],
    config: &GameConfig,
    window: Option<&Window>,
) -> Vec<NoteField> {
    let visible_width = window
        .map(|window| visible_world_size(window).x)
        .unwrap_or(SCREEN_SIZE.x);
    let columns: usize = specs.iter().map(|spec| spec.columns).sum();
    let gap_units = config.stage.field_gap_columns * (specs.len() - 1) as f32;
    let arrow_size = fitted_arrow_size(
        columns as f32 + gap_units,
        visible_width - 2.0 * config.stage.margin_x,
        max_arrow_size(config, window),
    );

    let mut layouts: Vec<NoteField> = specs
        .iter()
        .enumerate()
        .map(|(lane, spec)| NoteField {
            player: spec.player,
            lane,
            origin_x: 0.0,
            columns: spec.columns,
            speed: spec.speed,
            arrow_size,
            view: default(),
        })
        .collect();
    let gap = config.stage.field_gap_columns * layouts[0].spacing();
    let total: f32 =
        layouts.iter().map(NoteField::width).sum::<f32>() + gap * (layouts.len() - 1) as f32;
    let mut x = -total / 2.0;
    for layout in &mut layouts {
        layout.origin_x = x + layout.width() / 2.0;
        x += layout.width() + gap;
    }
    layouts
}

/// Re-packs the stage whenever the window changes: the arrow-size cap is
/// a screen-pixel budget, so a resize re-derives every field's arrow size
/// and origin, and the note-field systems move the lanes accordingly.
fn refit_stages_to_window(
    config: Res<GameConfig>,
    windows: Query<&Window, Changed<Window>>,
    mut fields: Query<&mut NoteField>,
) {
    let Ok(window) = windows.single() else { return };
    let mut current: Vec<Mut<NoteField>> = fields.iter_mut().collect();
    current.sort_by_key(|field| field.lane);
    let specs: Vec<PackSpec> = current
        .iter()
        .map(|field| PackSpec {
            player: field.player,
            columns: field.columns,
            speed: field.speed,
        })
        .collect();
    if specs.is_empty() {
        return;
    }
    for (field, packed) in current
        .iter_mut()
        .zip(pack_stage_fields(&specs, &config, Some(window)))
    {
        if field.arrow_size != packed.arrow_size || field.origin_x != packed.origin_x {
            field.arrow_size = packed.arrow_size;
            field.origin_x = packed.origin_x;
        }
    }
}

fn player_index(player: PlayerId) -> usize {
    match player {
        PlayerId::P1 => 0,
        PlayerId::P2 => 1,
    }
}

/// Spawns the music (when the stepfile has any) and the pre-rendered tick
/// track, both paused until the lead-in ends. The ticks cover every
/// stage's rows, so versus hears both charts.
fn spawn_audio_tracks(
    commands: &mut Commands,
    assets: &mut StageAssets,
    entry: &StepfileEntry,
    tick_times: &[Seconds],
    config: &GameConfig,
) {
    if let Some(path) = entry.music_path().as_deref().and_then(asset_server_path) {
        let music = assets.asset_server.load(path);
        commands
            .spawn_scoped(
                GameScene::Play,
                bsn! {
                    MusicTrack
                },
            )
            .insert(SoundPlayer {
                sound: music,
                options: SoundOptions {
                    paused: true,
                    volume: assets.machine.volume.music_gain(),
                    ..default()
                },
            });
    } else {
        info!(
            "no music file for \"{}\", playing silent",
            entry.display_title()
        );
    }

    match render_tick_track(
        &asset_root().join(Sfx::Tick.asset_path()),
        tick_times,
        config.tick_volume,
    ) {
        Ok(bytes) => {
            let handle = assets.sounds.add(Sound {
                bytes: bytes.into(),
            });
            commands
                .spawn_scoped(
                    GameScene::Play,
                    bsn! {
                        TickTrack
                    },
                )
                .insert(SoundPlayer {
                    sound: handle,
                    options: SoundOptions {
                        paused: true,
                        muted: true,
                        volume: assets.machine.volume.sfx_gain(),
                        ..default()
                    },
                });
        }
        Err(error) => warn!("could not render tick track: {error}"),
    }
}

/// Keeps the receptor arrows' top edge the configured screen-edge padding
/// below the window's top edge — the same breathing room the health vials
/// keep to their side — whatever extra world a non-16:9 window reveals
/// and whatever size the arrows were fitted to.
fn anchor_stage_to_window(
    config: Res<GameConfig>,
    windows: Query<&Window>,
    fields: Query<&NoteField>,
    mut clock: ResMut<NoteFieldClock>,
) {
    let Ok(window) = windows.single() else { return };
    let Some(arrow_size) = fields.iter().map(|field| field.arrow_size).reduce(f32::max) else {
        return;
    };
    let visible_top = visible_world_size(window).y / 2.0;
    let target_y = visible_top - config.stage.screen_edge_padding - arrow_size / 2.0;
    if clock.target_y != target_y {
        clock.target_y = target_y;
    }
}

/// Feeds each vial its player's health and the shared beat, so the vials
/// pulse with the music the arrows animate on.
fn sync_health_vials(
    session: Res<PlaySession>,
    clock: Res<NoteFieldClock>,
    mut vials: Query<(&mut HealthVial, &ForPlayer)>,
) {
    let beat = clock.beat();
    for (mut vial, owner) in &mut vials {
        let Some(stage) = session
            .stages()
            .iter()
            .find(|stage| stage.player == owner.0)
        else {
            continue;
        };
        vial.fill = stage.health_fraction();
        vial.beat = beat;
    }
}

/// Publishes the play stage's [`grade_text::GradeArea`] from the padded
/// window, so grades map their height option to the screen.
fn set_stage_grade_area(config: Res<GameConfig>, windows: Query<&Window>, mut commands: Commands) {
    let Ok(window) = windows.single() else {
        return;
    };
    let half = visible_world_size(window).y / 2.0;
    let padding = config.stage.screen_edge_padding;
    commands.insert_resource(grade_text::grade_area(half - padding, -half + padding));
}

/// The fail sting fires per failed stage (the engine already fades the
/// side out); once every stage is down the session ends and the grades
/// given so far become the final result.
fn react_to_failures(
    mut failures: MessageReader<StageFailed>,
    session: Res<PlaySession>,
    selected: Res<SelectedStepfile>,
    mut playback: ResMut<Playback>,
    mut sfx: MessageWriter<PlaySfx>,
    mut commands: Commands,
    mut fade: ResMut<SceneFade>,
) {
    let mut any = false;
    for _ in failures.read() {
        any = true;
        sfx.write(PlaySfx(Sfx::Fail));
    }
    if any && !playback.finished && session.all_failed() {
        playback.finished = true;
        commands.insert_resource(collect_results(&session, &selected, &playback));
        fade.begin(GameScene::Score);
    }
}

fn finish_when_complete(
    session: Res<PlaySession>,
    selected: Res<SelectedStepfile>,
    mut playback: ResMut<Playback>,
    music: Query<&SoundChannel, With<MusicTrack>>,
    tick: Query<&SoundChannel, With<TickTrack>>,
    mut commands: Commands,
    mut fade: ResMut<SceneFade>,
) {
    if playback.finished || !session.all_settled() {
        return;
    }
    let audio_done = if let Ok(channel) = music.single() {
        channel.is_finished()
    } else if let Ok(channel) = tick.single() {
        channel.is_finished()
    } else {
        playback.music.position().0 > playback.last_note_time.0 + 2.0
    };
    // Trailing mines and hold tails can outlive the audio; let them resolve.
    let chart_done = playback.music.position().0 >= playback.last_note_time.0;
    if !audio_done || !chart_done || !matches!(playback.phase, PlayPhase::Playing) {
        return;
    }
    playback.finished = true;
    commands.insert_resource(collect_results(&session, &selected, &playback));
    fade.begin(GameScene::Score);
}

fn collect_results(
    session: &PlaySession,
    selected: &SelectedStepfile,
    playback: &Playback,
) -> ScoreResults {
    let players = session
        .results()
        .into_iter()
        .zip(&selected.charts)
        .map(|(stage, player_chart)| PlayerResult {
            chart: player_chart.chart,
            stage,
        })
        .collect();
    ScoreResults {
        id: selected.id,
        title: playback.title.clone(),
        players,
    }
}

fn handle_cancel(
    actions: Actions,
    session: Res<PlaySession>,
    selected: Res<SelectedStepfile>,
    mut commands: Commands,
    mut fade: ResMut<SceneFade>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    let cancelled = session
        .stages()
        .iter()
        .any(|stage| actions.just_pressed(GameAction::cancel(stage.player)));
    if cancelled {
        sfx.write(PlaySfx(Sfx::Cancel));
        commands.insert_resource(WheelTarget(selected.id));
        fade.begin(GameScene::Wheel);
    }
}
