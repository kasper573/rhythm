mod background;
mod clock;
mod grading;
mod visuals;

use crate::core::assets::{asset_root, asset_server_path};
use crate::core::at;
use crate::core::config::{GameConfig, RowOutcome};
use crate::core::font::game_font;
use crate::core::health_vial::{HealthVialMaterial, spawn_health_vial};
use crate::core::input::{Actions, GameAction, shift_held};
use crate::core::library::{StepfileEntry, StepfileId, StepfileLibrary};
use crate::core::note_field::{
    NoteFieldClock, NoteFieldSystems, NoteSpawn, TARGET_Y, spawn_mine, spawn_note, spawn_receptors,
};
use crate::core::note_skin::ActiveNoteSkin;
use crate::core::scene_flow::SpawnScoped;
use crate::core::settings::{Settings, TimingSettings};
use crate::core::sfx::{PlaySfx, Sfx};
use crate::core::stepfile::{Chart, Difficulty, StepfileClock, StepfileTiming};
use crate::core::tick_track::render_tick_track;
use crate::core::units::{Millis, Seconds};
use crate::scenes::file_select::{FileSelectTarget, SelectedStepfile};
use crate::scenes::{GameScene, SceneFade, scene_accepts_input};
use bevy::audio::{AudioSinkPlayback, PlaybackMode};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

/// Grades are derived from the raw outcomes by whoever displays them.
#[derive(Resource, Debug, Clone)]
pub struct ScoreResults {
    pub id: StepfileId,
    pub title: String,
    pub result: PlayResult,
    pub difficulty: Difficulty,
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

/// How a play session ended: cleared the chart, or drained to zero health
/// partway through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayResult {
    Cleared,
    Failed,
}

pub struct FilePlayerPlugin;

impl Plugin for FilePlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<RowGraded>()
            .add_plugins((
                clock::plugin,
                grading::plugin,
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
                    toggle_tick_audio,
                    toggle_autosync,
                    fold_autosync,
                    update_autosync_status,
                    adjust_timing_offsets,
                )
                    .chain()
                    .in_set(PlaySet::Tune),
            )
            .add_systems(
                Update,
                (
                    fail_when_drained,
                    finish_when_complete,
                    handle_cancel.run_if(scene_accepts_input),
                )
                    .chain()
                    .in_set(PlaySet::Present),
            );
    }
}

/// The frame pipeline around the note field: the phases through `Sync` feed
/// it and run before [`NoteFieldSystems`]; `Present` reacts to the graded
/// frame after it.
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
    skin: Res<'w, ActiveNoteSkin>,
    asset_server: Res<'w, AssetServer>,
    audio_sources: ResMut<'w, Assets<AudioSource>>,
    vial_materials: ResMut<'w, Assets<HealthVialMaterial>>,
}

const LEAD_IN: Seconds = Seconds(2.0);

#[derive(Resource)]
pub(super) struct PlaySession {
    pub title: String,
    pub difficulty: Difficulty,
    pub rows: Vec<SessionRow>,
    pub mines: Vec<SessionMine>,
    pub graded_count: usize,
    pub expire_cursor: usize,
    pub clock: PlaybackClock,
    pub combo: u32,
    pub max_combo: u32,
    pub health: u32,
    pub last_note_time: Seconds,
    pub finished: bool,
    pub autosync: AutoSync,
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
    /// 1..=4 arrows; two or more make the row a jump.
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

/// Announces a graded row so the grade/combo displays can react.
#[derive(Message)]
pub(super) struct RowGraded {
    pub outcome: RowOutcome,
    pub combo: u32,
}

#[derive(Component, Default, Clone)]
pub(super) struct MusicTrack;

/// The pre-rendered tick track sink, always playing in sync, muted unless
/// tick audio is toggled on.
#[derive(Component, Default, Clone)]
pub(super) struct TickTrack;

#[derive(Component, Default, Clone)]
pub(super) struct GradeText;

#[derive(Component, Default, Clone)]
pub(super) struct ComboText;

#[derive(Component, Default, Clone)]
pub(super) struct OffsetOsd;

#[derive(Component, Default, Clone)]
pub(super) struct AutoSyncText;

fn enter(
    mut commands: Commands,
    selected: Option<Res<SelectedStepfile>>,
    library: Res<StepfileLibrary>,
    config: Res<GameConfig>,
    settings: Res<Settings>,
    mut assets: StageAssets,
    mut fade: ResMut<SceneFade>,
) {
    let Some(selected) = selected else {
        fade.begin(GameScene::FileSelect);
        return;
    };
    let entry = library.stepfile(selected.id);
    let chart = entry
        .stepfile
        .charts
        .get(selected.chart)
        .or_else(|| entry.stepfile.preferred_chart());
    let Some(chart) = chart else {
        fade.begin(GameScene::FileSelect);
        return;
    };
    if chart.columns != 4 {
        warn!(
            "chart has {} columns; only 4-column play is supported",
            chart.columns
        );
    }

    let timing = entry.stepfile.timing.clone();

    for entity in spawn_receptors(&mut commands, &assets.skin) {
        commands
            .entity(entity)
            .insert(DespawnOnExit(GameScene::FilePlayer));
    }
    let vial = spawn_health_vial(&mut commands, &mut assets.vial_materials, 1.0);
    commands
        .entity(vial)
        .insert(DespawnOnExit(GameScene::FilePlayer));

    let (rows, mines, last_note_time) =
        spawn_chart(&mut commands, &assets.skin, &config, chart, &timing);
    spawn_audio_tracks(&mut commands, &mut assets, entry, &rows, &config);
    background::spawn_background(&mut commands, entry, &timing);
    spawn_hud(&mut commands);

    commands.insert_resource(NoteFieldClock {
        visible: -LEAD_IN,
        timing: timing.clone(),
        speed: settings.stepfile.note_speed,
        target_y: TARGET_Y,
    });
    commands.insert_resource(PlaySession {
        title: entry.display_title(),
        difficulty: chart.difficulty.clone(),
        rows,
        mines,
        graded_count: 0,
        expire_cursor: 0,
        clock: PlaybackClock {
            phase: PlayPhase::LeadIn { remaining: LEAD_IN },
            music: StepfileClock::start_at(timing, -LEAD_IN),
            wall_since_play: Seconds::ZERO,
            latency_samples: Vec::new(),
        },
        combo: 0,
        max_combo: 0,
        health: config.player_max_health,
        last_note_time,
        finished: false,
        autosync: AutoSync::default(),
    });
}

fn exit(mut commands: Commands) {
    commands.remove_resource::<PlaySession>();
    commands.remove_resource::<NoteFieldClock>();
    commands.remove_resource::<SelectedStepfile>();
}

/// Spawns every note and mine of the chart into the field, scoped to the
/// scene, and returns the session records tracking them plus the time the
/// chart is over.
fn spawn_chart(
    commands: &mut Commands,
    skin: &ActiveNoteSkin,
    config: &GameConfig,
    chart: &Chart,
    timing: &StepfileTiming,
) -> (Vec<SessionRow>, Vec<SessionMine>, Seconds) {
    let mut mines = Vec::new();
    let mut last_note_time = Seconds::ZERO;
    for mine in &chart.mines {
        if mine.column >= 4 {
            continue;
        }
        let time = timing.seconds_at_beat(mine.beat);
        last_note_time = last_note_time.max(time);
        let entity = spawn_mine(commands, skin, time, mine.beat, mine.column);
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
            if arrow.column >= 4 {
                continue;
            }
            let end = arrow
                .tail
                .map(|tail| (timing.seconds_at_beat(tail.end), tail.end));
            last_note_time = last_note_time.max(end.map(|(end, _)| end).unwrap_or(time));
            let spawned = spawn_note(
                commands,
                skin,
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
        if !arrows.is_empty() {
            rows.push(SessionRow {
                time,
                outcome: None,
                arrows,
            });
        }
    }
    // Warps and stops can reorder wall-clock times relative to beats; the
    // expiry cursor needs time order.
    rows.sort_by(|a, b| a.time.0.total_cmp(&b.time.0));
    (rows, mines, last_note_time)
}

/// Spawns the music (when the stepfile has any) and the pre-rendered tick
/// track, both paused until the lead-in ends.
fn spawn_audio_tracks(
    commands: &mut Commands,
    assets: &mut StageAssets,
    entry: &StepfileEntry,
    rows: &[SessionRow],
    config: &GameConfig,
) {
    if let Some(path) = entry.music_path().as_deref().and_then(asset_server_path) {
        let music = assets.asset_server.load(path);
        commands.spawn_scoped(
            GameScene::FilePlayer,
            bsn! {
                MusicTrack
                AudioPlayer({music})
                PlaybackSettings {
                    mode: {PlaybackMode::Once},
                    paused: true,
                }
            },
        );
    } else {
        info!(
            "no music file for \"{}\", playing silent",
            entry.display_title()
        );
    }

    let tick_times: Vec<Seconds> = rows.iter().map(|row| row.time).collect();
    match render_tick_track(
        &asset_root().join(Sfx::Tick.asset_path()),
        &tick_times,
        config.tick_volume,
    ) {
        Ok(bytes) => {
            let handle = assets.audio_sources.add(AudioSource {
                bytes: bytes.into(),
            });
            commands.spawn_scoped(
                GameScene::FilePlayer,
                bsn! {
                    TickTrack
                    AudioPlayer({handle})
                    PlaybackSettings {
                        mode: {PlaybackMode::Once},
                        paused: true,
                        muted: true,
                    }
                },
            );
        }
        Err(error) => warn!("could not render tick track: {error}"),
    }
}

fn spawn_hud(commands: &mut Commands) {
    commands.spawn_scoped(
        GameScene::FilePlayer,
        bsn! {
            ComboText
            game_font(44.0)
            Text2d("")
            TextColor(Color::WHITE)
            at(0.0, -60.0, 5.0)
            Visibility::Hidden
        },
    );
    commands.spawn_scoped(
        GameScene::FilePlayer,
        bsn! {
            GradeText
            game_font(50.0)
            Text2d("")
            TextColor(Color::srgba(1.0, 1.0, 1.0, 0.0))
            at(0.0, 10.0, 6.0)
        },
    );
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

fn toggle_autosync(actions: Actions, mut session: ResMut<PlaySession>) {
    if !actions.just_pressed(GameAction::ToggleAutoSync) {
        return;
    }
    session.autosync.enabled = !session.autosync.enabled;
    session.autosync.samples.clear();
}

/// AutoSync: with enough hit samples, fold their median error into the
/// machine offset (surfacing it through the usual offset OSD), reset, and
/// keep collecting until toggled off.
const AUTOSYNC_SAMPLES: usize = 24;

fn fold_autosync(
    mut session: ResMut<PlaySession>,
    mut settings: ResMut<Settings>,
    mut osd: MessageWriter<visuals::OffsetOsdLine>,
) {
    if !session.autosync.enabled || session.autosync.samples.len() < AUTOSYNC_SAMPLES {
        return;
    }
    let mut samples = std::mem::take(&mut session.autosync.samples);
    samples.sort_by(|a, b| a.0.total_cmp(&b.0));
    let median = samples[samples.len() / 2];
    let delta = Millis(median.to_millis().round() as i64);
    if delta == Millis(0) {
        return;
    }
    settings.timing.machine_offset = settings.timing.machine_offset + delta;
    osd.write(visuals::OffsetOsdLine(format!(
        "Machine offset: {}",
        settings.timing.machine_offset
    )));
}

fn update_autosync_status(
    session: Res<PlaySession>,
    mut status: Single<(&mut Text, &mut Visibility), With<AutoSyncText>>,
    mut shown: Local<Option<(bool, usize)>>,
) {
    let state = (session.autosync.enabled, session.autosync.samples.len());
    if *shown == Some(state) {
        return;
    }
    *shown = Some(state);
    let (text, visibility) = &mut *status;
    if session.autosync.enabled {
        text.0 = format!("AutoSync ({}/{AUTOSYNC_SAMPLES} samples)", state.1);
        **visibility = Visibility::Visible;
    } else {
        **visibility = Visibility::Hidden;
    }
}

fn toggle_tick_audio(actions: Actions, mut tick: Query<&mut AudioSink, With<TickTrack>>) {
    if !actions.just_pressed(GameAction::ToggleTickAudio) {
        return;
    }
    for mut sink in &mut tick {
        sink.toggle_mute();
    }
}

/// Adjusts the three synchronization offsets by 1ms (10ms with SHIFT held)
/// and surfaces the new value on the OSD.
fn adjust_timing_offsets(
    keys: Res<ButtonInput<KeyCode>>,
    mut settings: ResMut<Settings>,
    config: Res<GameConfig>,
    mut osd: MessageWriter<visuals::OffsetOsdLine>,
) {
    let step = if shift_held(&keys) { 10 } else { 1 };
    let pairs = [
        (
            GameAction::DecreaseMachineOffset,
            GameAction::IncreaseMachineOffset,
        ),
        (
            GameAction::DecreaseVisualDelay,
            GameAction::IncreaseVisualDelay,
        ),
        (
            GameAction::DecreaseAudioLatency,
            GameAction::IncreaseAudioLatency,
        ),
    ];
    let mut osd_line = None;
    for (index, (decrease, increase)) in pairs.into_iter().enumerate() {
        let mut delta: i64 = 0;
        if settings.keymap.just_pressed(&keys, increase, &config) {
            delta += step;
        }
        if settings.keymap.just_pressed(&keys, decrease, &config) {
            delta -= step;
        }
        if delta == 0 {
            continue;
        }
        let timing = &mut settings.timing;
        osd_line = Some(match index {
            0 => {
                timing.machine_offset = timing.machine_offset + Millis(delta);
                format!("Machine offset: {}", timing.machine_offset)
            }
            1 => {
                timing.visual_delay = timing.visual_delay + Millis(delta);
                format!("Visual delay: {}", timing.visual_delay)
            }
            _ => {
                let latency = timing.audio_latency() + Millis(delta);
                timing.audio_latency = Some(latency);
                format!("Audio latency: {latency}")
            }
        });
    }
    let Some(line) = osd_line else { return };
    osd.write(visuals::OffsetOsdLine(line));
}

fn finish_when_complete(
    mut session: ResMut<PlaySession>,
    selected: Res<SelectedStepfile>,
    music: Query<&AudioSink, With<MusicTrack>>,
    tick: Query<&AudioSink, With<TickTrack>>,
    mut commands: Commands,
    mut fade: ResMut<SceneFade>,
) {
    if session.finished || session.graded_count < session.rows.len() {
        return;
    }
    let audio_done = if let Ok(sink) = music.single() {
        sink.empty()
    } else if let Ok(sink) = tick.single() {
        sink.empty()
    } else {
        session.clock.music.position().0 > session.last_note_time.0 + 2.0
    };
    // Trailing mines and hold tails can outlive the audio; let them resolve.
    let chart_done = session.clock.music.position().0 >= session.last_note_time.0;
    if !audio_done || !chart_done || !matches!(session.clock.phase, PlayPhase::Playing) {
        return;
    }
    session.finished = true;
    commands.insert_resource(collect_results(&session, &selected, PlayResult::Cleared));
    fade.begin(GameScene::Score);
}

/// Zero health ends the session on the spot: the fail sting fires here so
/// it plays through the transition, and the grades given so far become the
/// final result.
fn fail_when_drained(
    mut session: ResMut<PlaySession>,
    selected: Res<SelectedStepfile>,
    mut sfx: MessageWriter<PlaySfx>,
    mut commands: Commands,
    mut fade: ResMut<SceneFade>,
) {
    if session.finished || session.health > 0 {
        return;
    }
    session.finished = true;
    sfx.write(PlaySfx(Sfx::Fail));
    commands.insert_resource(collect_results(&session, &selected, PlayResult::Failed));
    fade.begin(GameScene::Score);
}

fn collect_results(
    session: &PlaySession,
    selected: &SelectedStepfile,
    result: PlayResult,
) -> ScoreResults {
    let holds: Vec<&HoldState> = session
        .rows
        .iter()
        .flat_map(|row| &row.arrows)
        .filter_map(|arrow| arrow.hold.as_ref())
        .collect();
    ScoreResults {
        id: selected.id,
        title: session.title.clone(),
        result,
        difficulty: session.difficulty.clone(),
        outcomes: session.rows.iter().filter_map(|row| row.outcome).collect(),
        rows_total: session.rows.len() as u32,
        max_combo: session.max_combo,
        holds_ok: holds
            .iter()
            .filter(|hold| hold.result == Some(HoldOutcome::Ok))
            .count() as u32,
        holds_ng: holds
            .iter()
            .filter(|hold| hold.result == Some(HoldOutcome::Ng))
            .count() as u32,
        holds_total: holds.len() as u32,
        mines_exploded: session
            .mines
            .iter()
            .filter(|mine| mine.outcome == Some(MineOutcome::Exploded))
            .count() as u32,
        mines_total: session.mines.len() as u32,
    }
}

fn handle_cancel(
    actions: Actions,
    selected: Res<SelectedStepfile>,
    mut commands: Commands,
    mut fade: ResMut<SceneFade>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    if actions.just_pressed(GameAction::Cancel) {
        sfx.write(PlaySfx(Sfx::Cancel));
        commands.insert_resource(FileSelectTarget::Stepfile(selected.id));
        fade.begin(GameScene::FileSelect);
    }
}

pub(super) fn direction_action(column: usize) -> GameAction {
    match column {
        0 => GameAction::Left,
        1 => GameAction::Down,
        2 => GameAction::Up,
        _ => GameAction::Right,
    }
}
