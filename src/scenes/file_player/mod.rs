mod background;
mod clock;
mod grading;
mod tuning;
mod visuals;

use crate::core::assets::{asset_root, asset_server_path};
use crate::core::audio::{Sound, SoundChannel, SoundPlayer};
use crate::core::config::{GameConfig, RowOutcome};
use crate::core::font::game_font;
use crate::core::health_vial::{HealthVialMaterial, VialSide, spawn_health_vial};
use crate::core::input::{Actions, GameAction};
use crate::core::library::{StepfileEntry, StepfileId, StepfileLibrary};
use crate::core::note_field::{
    FadeOut, InField, NoteField, NoteFieldClock, NoteFieldSystems, NoteSpawn, TARGET_Y,
    fitted_arrow_size, spawn_mine, spawn_note, spawn_note_field, spawn_receptors,
};
use crate::core::note_skin::{ActiveNoteSkin, ActiveNoteSkins};
use crate::core::platform::SoundOptions;
use crate::core::player::PlayerId;
use crate::core::scene_flow::SpawnScoped;
use crate::core::settings::{PlayerSettings, TimingSettings};
use crate::core::sfx::{PlaySfx, Sfx};
use crate::core::stepfile::{Chart, MusicPlayer, StepfileClock, StepfileTiming};
use crate::core::tick_track::render_tick_track;
use crate::core::units::Seconds;
use crate::core::{SCREEN_SIZE, at};
use crate::scenes::file_select::{FileSelectTarget, SelectedStepfile};
use crate::scenes::{GameScene, SceneFade, scene_accepts_input};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

/// Grades are derived from the raw outcomes by whoever displays them.
#[derive(Resource, Debug, Clone)]
pub struct ScoreResults {
    pub id: StepfileId,
    pub title: String,
    pub players: Vec<PlayerResult>,
}

/// One player's complete run.
#[derive(Debug, Clone)]
pub struct PlayerResult {
    pub player: PlayerId,
    /// The run drained to zero health before the chart ended.
    pub failed: bool,
    /// Index into the played stepfile's `charts`.
    pub chart: usize,
    pub outcomes: Vec<RowOutcome>,
    /// Every row of the chart, so partial (failed) runs still rate against
    /// the whole song.
    pub rows_total: u32,
    pub max_combo: u32,
    pub holds_ok: u32,
    pub holds_ng: u32,
    pub holds_total: u32,
    pub mines_exploded: u32,
    pub mines_total: u32,
}

pub(super) struct FilePlayerPlugin;

impl Plugin for FilePlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<RowGraded>()
            .add_plugins((
                clock::plugin,
                grading::plugin,
                tuning::plugin,
                visuals::plugin,
                background::plugin,
            ))
            .configure_sets(
                Update,
                (
                    (PlaySet::Clock, PlaySet::Grade, PlaySet::Tune, PlaySet::Sync)
                        .chain()
                        .before(NoteFieldSystems),
                    PlaySet::Present.after(NoteFieldSystems),
                )
                    .run_if(
                        in_state(GameScene::FilePlayer).and_then(resource_exists::<PlaySession>),
                    ),
            )
            .add_systems(OnEnter(GameScene::FilePlayer), enter)
            .add_systems(OnExit(GameScene::FilePlayer), exit)
            .add_systems(
                Update,
                (
                    fail_drained_stages,
                    finish_when_complete,
                    handle_cancel.run_if(scene_accepts_input),
                )
                    .chain()
                    .in_set(PlaySet::Present),
            );
    }
}

/// The frame pipeline around the note fields: the phases through `Sync`
/// feed them and run before [`NoteFieldSystems`]; `Present` reacts to the
/// graded frame after it.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum PlaySet {
    Clock,
    Grade,
    Tune,
    Sync,
    Present,
}

/// The sources everything on stage is materialized from.
#[derive(SystemParam)]
struct StageAssets<'w> {
    skins: Res<'w, ActiveNoteSkins>,
    asset_server: Res<'w, AssetServer>,
    sounds: ResMut<'w, Assets<Sound>>,
    vial_materials: ResMut<'w, Assets<HealthVialMaterial>>,
}

const LEAD_IN: Seconds = Seconds(2.0);

/// Width reserved on each screen edge that fields never enter: the health
/// vials plus breathing room.
const STAGE_MARGIN_X: f32 = 150.0;

/// Gap between adjacent fields, in column spacings.
const FIELD_GAP_COLUMNS: f32 = 2.0;

#[derive(Resource)]
pub(super) struct PlaySession {
    pub title: String,
    /// One per active player, in [`SelectedStepfile`] chart order; a
    /// doubles session is P1's single wide stage.
    pub stages: Vec<Stage>,
    pub clock: PlaybackClock,
    pub last_note_time: Seconds,
    pub finished: bool,
    pub autosync: AutoSync,
}

/// One player's run through their chart: their notes, their grading
/// cursors, their combo and health. The field's geometry and input
/// mapping live on the [`NoteField`] component of `field`. Draining to
/// zero fails only this stage; the session ends when every stage has
/// failed or every surviving one is complete.
pub(super) struct Stage {
    pub player: PlayerId,
    pub field: Entity,
    pub rows: Vec<SessionRow>,
    pub mines: Vec<SessionMine>,
    pub graded_count: usize,
    pub expire_cursor: usize,
    pub combo: u32,
    pub max_combo: u32,
    pub health: u32,
    pub failed: bool,
}

impl Stage {
    pub fn complete(&self) -> bool {
        self.graded_count >= self.rows.len()
    }
}

/// The session's playback timeline, advanced by the [`clock`] module: the
/// lead-in phase, the shared stepfile music clock, and the state for the
/// first-play audio latency measurement.
pub(super) struct PlaybackClock {
    pub phase: PlayPhase,
    pub music: StepfileClock,
    /// Wall-clock time since the tracks were started, for measuring how far
    /// the mixer's queue runs ahead of real time (the audio latency).
    pub wall_since_play: Seconds,
    pub latency_samples: Vec<Seconds>,
}

pub(super) enum PlayPhase {
    LeadIn { remaining: Seconds },
    Playing,
}

/// While enabled, hit errors accumulate and the median of every batch is
/// folded into the machine offset (AutoSync).
#[derive(Default)]
pub(super) struct AutoSync {
    pub enabled: bool,
    pub samples: Vec<Seconds>,
}

impl PlaySession {
    pub fn graded_now(&self, timing: &TimingSettings) -> Seconds {
        self.clock.music.graded_now(timing)
    }

    pub fn visible_now(&self, timing: &TimingSettings) -> Seconds {
        self.clock.music.visible_now(timing)
    }
}

/// One row of the chart as played: every arrow in it must be stepped, and
/// the whole row resolves into a single outcome (see the grading systems).
pub(super) struct SessionRow {
    pub time: Seconds,
    pub outcome: Option<RowOutcome>,
    /// Simultaneous arrows; two or more make the row a jump.
    pub arrows: Vec<SessionArrow>,
}

impl SessionRow {
    /// Rows resolve once every arrow has a banked press.
    pub fn complete(&self) -> bool {
        self.arrows.iter().all(|arrow| arrow.error.is_some())
    }
}

pub(super) struct SessionArrow {
    pub column: usize,
    pub entity: Entity,
    /// The banked press: its timing error is locked in silently when the
    /// panel is stepped and only counts once the whole row resolves.
    pub error: Option<Seconds>,
    pub hold: Option<HoldState>,
}

/// Live state of one hold or roll; the life rules live in
/// [`grading::update_holds`].
pub(super) struct HoldState {
    pub end: Seconds,
    pub roll: bool,
    pub life: f32,
    /// The head was stepped on, activating the hold.
    pub engaged: bool,
    /// Whether the panel is currently satisfied (held, for holds).
    pub held_now: bool,
    pub result: Option<HoldOutcome>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HoldOutcome {
    /// Held to the end.
    Ok,
    /// Dropped, or the head was missed.
    Ng,
}

pub(super) struct SessionMine {
    pub time: Seconds,
    pub column: usize,
    pub entity: Entity,
    pub outcome: Option<MineOutcome>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MineOutcome {
    Exploded,
    Avoided,
}

/// Announces a stage's graded row so the grade/combo displays can react.
#[derive(Message)]
pub(super) struct RowGraded {
    pub player: PlayerId,
    pub outcome: RowOutcome,
    pub combo: u32,
}

#[derive(Component, Default, Clone)]
pub(super) struct MusicTrack;

/// The pre-rendered tick track sink, always playing in sync, muted unless
/// tick audio is toggled on.
#[derive(Component, Default, Clone)]
pub(super) struct TickTrack;

/// Tags a HUD element with the stage player it reports on. Public only
/// because the template derive demands it.
#[derive(Component, Clone, Copy, FromTemplate)]
pub struct ForPlayer(pub PlayerId);

#[derive(Component, Default, Clone)]
pub(super) struct GradeText;

/// The combo readout, carrying its own bounce animation state.
#[derive(Component, Default, Clone)]
pub(super) struct ComboText {
    pub bounce: Seconds,
    pub last_combo: u32,
}

#[derive(Component, Default, Clone)]
pub(super) struct OffsetOsd;

#[derive(Component, Default, Clone)]
pub(super) struct AutoSyncText;

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
        fade.begin(GameScene::FileSelect);
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
        fade.begin(GameScene::FileSelect);
        return;
    };
    let layouts = stage_layouts(&charts, &player_settings);

    let mut stages = Vec::new();
    let mut last_note_time = Seconds::ZERO;
    for ((player, chart), layout) in charts.into_iter().zip(layouts) {
        let origin_x = layout.origin_x;
        let skin = assets.skins.get(player);
        let field = spawn_note_field(&mut commands, layout.clone());
        commands
            .entity(field)
            .insert(DespawnOnExit(GameScene::FilePlayer));
        for entity in spawn_receptors(&mut commands, skin, field, &layout) {
            commands
                .entity(entity)
                .insert(DespawnOnExit(GameScene::FilePlayer));
        }
        let side = match player {
            PlayerId::P1 => VialSide::Left,
            PlayerId::P2 => VialSide::Right,
        };
        let vial = spawn_health_vial(&mut commands, &mut assets.vial_materials, 1.0, side);
        commands
            .entity(vial)
            .insert((ForPlayer(player), DespawnOnExit(GameScene::FilePlayer)));
        spawn_stage_hud(&mut commands, player, origin_x);

        let (rows, mines, stage_last) =
            spawn_chart(&mut commands, skin, field, &layout, &config, chart, &timing);
        last_note_time = last_note_time.max(stage_last);
        stages.push(Stage {
            player,
            field,
            rows,
            mines,
            graded_count: 0,
            expire_cursor: 0,
            combo: 0,
            max_combo: 0,
            health: config.player_max_health,
            failed: false,
        });
    }
    if stages.is_empty() {
        fade.begin(GameScene::FileSelect);
        return;
    }

    spawn_audio_tracks(&mut commands, &mut assets, entry, &stages, &config);
    background::spawn_background(&mut commands, entry, &timing);
    spawn_shared_hud(&mut commands);

    commands.insert_resource(NoteFieldClock {
        visible: -LEAD_IN,
        timing: timing.clone(),
        target_y: TARGET_Y,
    });
    commands.insert_resource(PlaySession {
        title: entry.display_title(),
        stages,
        clock: PlaybackClock {
            phase: PlayPhase::LeadIn { remaining: LEAD_IN },
            music: StepfileClock::start_at(timing, -LEAD_IN),
            wall_since_play: Seconds::ZERO,
            latency_samples: Vec::new(),
        },
        last_note_time,
        finished: false,
        autosync: AutoSync::default(),
    });
}

fn exit(mut commands: Commands, mut music: ResMut<MusicPlayer>) {
    music.stop();
    commands.remove_resource::<PlaySession>();
    commands.remove_resource::<NoteFieldClock>();
    commands.remove_resource::<SelectedStepfile>();
}

/// Sizes and places one field per chart: arrows grow to
/// [`MAX_ARROW_SIZE`](crate::core::note_field::MAX_ARROW_SIZE) when the
/// screen has room and shrink until every column — plus the gaps between
/// fields — fits between the reserved screen edges. The fields pack
/// left-to-right, centered as a block.
fn stage_layouts(
    charts: &[(PlayerId, &Chart)],
    player_settings: &PlayerSettings,
) -> Vec<NoteField> {
    let columns: usize = charts.iter().map(|(_, chart)| chart.columns).sum();
    let gap_units = FIELD_GAP_COLUMNS * (charts.len() - 1) as f32;
    let arrow_size = fitted_arrow_size(
        columns as f32 + gap_units,
        SCREEN_SIZE.x - 2.0 * STAGE_MARGIN_X,
    );

    let mut layouts: Vec<NoteField> = charts
        .iter()
        .map(|(player, chart)| NoteField {
            player: *player,
            origin_x: 0.0,
            columns: chart.columns,
            speed: player_settings[*player].note_speed,
            arrow_size,
        })
        .collect();
    let gap = FIELD_GAP_COLUMNS * layouts[0].spacing();
    let total: f32 =
        layouts.iter().map(NoteField::width).sum::<f32>() + gap * (layouts.len() - 1) as f32;
    let mut x = -total / 2.0;
    for layout in &mut layouts {
        layout.origin_x = x + layout.width() / 2.0;
        x += layout.width() + gap;
    }
    layouts
}

/// Spawns every note and mine of the chart into the stage's field, scoped
/// to the scene, and returns the session records tracking them plus the
/// time the chart is over.
fn spawn_chart(
    commands: &mut Commands,
    skin: &ActiveNoteSkin,
    field: Entity,
    layout: &NoteField,
    config: &GameConfig,
    chart: &Chart,
    timing: &StepfileTiming,
) -> (Vec<SessionRow>, Vec<SessionMine>, Seconds) {
    let mut mines = Vec::new();
    let mut last_note_time = Seconds::ZERO;
    for mine in &chart.mines {
        let time = timing.seconds_at_beat(mine.beat);
        last_note_time = last_note_time.max(time);
        let entity = spawn_mine(commands, skin, field, layout, time, mine.beat, mine.column);
        commands
            .entity(entity)
            .insert(DespawnOnExit(GameScene::FilePlayer));
        mines.push(SessionMine {
            time,
            column: mine.column,
            entity,
            outcome: None,
        });
    }

    let mut rows = Vec::new();
    for row in &chart.rows {
        let time = timing.seconds_at_beat(row.beat);
        let quant = config.recognized_quant(row.quant);
        let mut arrows = Vec::new();
        for arrow in &row.arrows {
            let end = arrow
                .tail
                .map(|tail| (timing.seconds_at_beat(tail.end), tail.end));
            last_note_time = last_note_time.max(end.map(|(end, _)| end).unwrap_or(time));
            let spawned = spawn_note(
                commands,
                skin,
                field,
                layout,
                &NoteSpawn {
                    time,
                    beat: row.beat,
                    column: arrow.column,
                    quant,
                    end,
                },
            );
            for entity in spawned.entities() {
                commands
                    .entity(entity)
                    .insert(DespawnOnExit(GameScene::FilePlayer));
            }
            arrows.push(SessionArrow {
                column: arrow.column,
                entity: spawned.head,
                error: None,
                hold: arrow.tail.map(|tail| HoldState {
                    end: timing.seconds_at_beat(tail.end),
                    roll: tail.roll,
                    life: 1.0,
                    engaged: false,
                    held_now: false,
                    result: None,
                }),
            });
        }
        rows.push(SessionRow {
            time,
            outcome: None,
            arrows,
        });
    }
    // Warps and stops can reorder wall-clock times relative to beats; the
    // expiry cursor needs time order.
    rows.sort_by(|a, b| a.time.0.total_cmp(&b.time.0));
    (rows, mines, last_note_time)
}

/// Spawns the music (when the stepfile has any) and the pre-rendered tick
/// track, both paused until the lead-in ends. The ticks cover every
/// stage's rows, so versus hears both charts.
fn spawn_audio_tracks(
    commands: &mut Commands,
    assets: &mut StageAssets,
    entry: &StepfileEntry,
    stages: &[Stage],
    config: &GameConfig,
) {
    if let Some(path) = entry.music_path().as_deref().and_then(asset_server_path) {
        let music = assets.asset_server.load(path);
        commands
            .spawn_scoped(
                GameScene::FilePlayer,
                bsn! {
                    MusicTrack
                },
            )
            .insert(SoundPlayer {
                sound: music,
                options: SoundOptions {
                    paused: true,
                    ..default()
                },
            });
    } else {
        info!(
            "no music file for \"{}\", playing silent",
            entry.display_title()
        );
    }

    let tick_times: Vec<Seconds> = stages
        .iter()
        .flat_map(|stage| stage.rows.iter().map(|row| row.time))
        .collect();
    match render_tick_track(
        &asset_root().join(Sfx::Tick.asset_path()),
        &tick_times,
        config.tick_volume,
    ) {
        Ok(bytes) => {
            let handle = assets.sounds.add(Sound {
                bytes: bytes.into(),
            });
            commands
                .spawn_scoped(
                    GameScene::FilePlayer,
                    bsn! {
                        TickTrack
                    },
                )
                .insert(SoundPlayer {
                    sound: handle,
                    options: SoundOptions {
                        paused: true,
                        muted: true,
                        ..default()
                    },
                });
        }
        Err(error) => warn!("could not render tick track: {error}"),
    }
}

/// The per-stage readouts, centered over the stage's field.
fn spawn_stage_hud(commands: &mut Commands, player: PlayerId, origin_x: f32) {
    commands.spawn_scoped(
        GameScene::FilePlayer,
        bsn! {
            ComboText
            ForPlayer({player})
            game_font(44.0)
            Text2d("")
            TextColor(Color::WHITE)
            at(origin_x, -60.0, 5.0)
            Visibility::Hidden
        },
    );
    commands.spawn_scoped(
        GameScene::FilePlayer,
        bsn! {
            GradeText
            ForPlayer({player})
            game_font(50.0)
            Text2d("")
            TextColor(Color::srgba(1.0, 1.0, 1.0, 0.0))
            at(origin_x, 10.0, 6.0)
        },
    );
}

/// The machine-wide readouts: the timing-offset OSD and AutoSync status.
fn spawn_shared_hud(commands: &mut Commands) {
    commands.spawn_scoped(
        GameScene::FilePlayer,
        bsn! {
            OffsetOsd
            game_font(24.0)
            Text("")
            TextColor(Color::srgba(1.0, 1.0, 1.0, 0.0))
            Node {
                position_type: PositionType::Absolute,
                right: px(24),
                bottom: px(16),
            }
        },
    );
    commands.spawn_scoped(
        GameScene::FilePlayer,
        bsn! {
            AutoSyncText
            game_font(24.0)
            Text("")
            TextColor(Color::srgb(0.5, 0.9, 1.0))
            Node {
                position_type: PositionType::Absolute,
                right: px(24),
                bottom: px(48),
            }
            Visibility::Hidden
        },
    );
}

fn finish_when_complete(
    mut session: ResMut<PlaySession>,
    selected: Res<SelectedStepfile>,
    music: Query<&SoundChannel, With<MusicTrack>>,
    tick: Query<&SoundChannel, With<TickTrack>>,
    mut commands: Commands,
    mut fade: ResMut<SceneFade>,
) {
    let all_settled = session
        .stages
        .iter()
        .all(|stage| stage.failed || stage.complete());
    if session.finished || !all_settled {
        return;
    }
    let audio_done = if let Ok(channel) = music.single() {
        channel.is_finished()
    } else if let Ok(channel) = tick.single() {
        channel.is_finished()
    } else {
        session.clock.music.position().0 > session.last_note_time.0 + 2.0
    };
    // Trailing mines and hold tails can outlive the audio; let them resolve.
    let chart_done = session.clock.music.position().0 >= session.last_note_time.0;
    if !audio_done || !chart_done || !matches!(session.clock.phase, PlayPhase::Playing) {
        return;
    }
    session.finished = true;
    commands.insert_resource(collect_results(&session, &selected));
    fade.begin(GameScene::Score);
}

/// Zero health fails that stage on the spot: its remaining notes fade out
/// and its grading stops, while any surviving stage plays on. The fail
/// sting fires per failed stage; once every stage is down the session
/// ends and the grades given so far become the final result.
fn fail_drained_stages(
    mut session: ResMut<PlaySession>,
    selected: Res<SelectedStepfile>,
    staged: Query<(Entity, &InField)>,
    mut sfx: MessageWriter<PlaySfx>,
    mut commands: Commands,
    mut fade: ResMut<SceneFade>,
) {
    if session.finished {
        return;
    }
    for stage in &mut session.stages {
        if stage.failed || stage.health > 0 {
            continue;
        }
        stage.failed = true;
        sfx.write(PlaySfx(Sfx::Fail));
        // The whole side shuts down: notes, mines, and receptors fade away.
        for (entity, in_field) in &staged {
            if in_field.0 == stage.field {
                commands
                    .entity(entity)
                    .insert(FadeOut::over(FAIL_FADE_SECONDS));
            }
        }
    }
    if session.stages.iter().all(|stage| stage.failed) {
        session.finished = true;
        commands.insert_resource(collect_results(&session, &selected));
        fade.begin(GameScene::Score);
    }
}

const FAIL_FADE_SECONDS: f32 = 0.8;

fn collect_results(session: &PlaySession, selected: &SelectedStepfile) -> ScoreResults {
    let players = session
        .stages
        .iter()
        .zip(&selected.charts)
        .map(|(stage, player_chart)| {
            let holds: Vec<&HoldState> = stage
                .rows
                .iter()
                .flat_map(|row| &row.arrows)
                .filter_map(|arrow| arrow.hold.as_ref())
                .collect();
            PlayerResult {
                player: stage.player,
                failed: stage.failed,
                chart: player_chart.chart,
                outcomes: stage.rows.iter().filter_map(|row| row.outcome).collect(),
                rows_total: stage.rows.len() as u32,
                max_combo: stage.max_combo,
                holds_ok: holds
                    .iter()
                    .filter(|hold| hold.result == Some(HoldOutcome::Ok))
                    .count() as u32,
                holds_ng: holds
                    .iter()
                    .filter(|hold| hold.result == Some(HoldOutcome::Ng))
                    .count() as u32,
                holds_total: holds.len() as u32,
                mines_exploded: stage
                    .mines
                    .iter()
                    .filter(|mine| mine.outcome == Some(MineOutcome::Exploded))
                    .count() as u32,
                mines_total: stage.mines.len() as u32,
            }
        })
        .collect();
    ScoreResults {
        id: selected.id,
        title: session.title.clone(),
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
        .stages
        .iter()
        .any(|stage| actions.just_pressed(GameAction::cancel(stage.player)));
    if cancelled {
        sfx.write(PlaySfx(Sfx::Cancel));
        commands.insert_resource(FileSelectTarget(selected.id));
        fade.begin(GameScene::FileSelect);
    }
}
