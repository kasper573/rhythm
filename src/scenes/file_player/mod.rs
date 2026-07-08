mod background;
mod clock;
mod grading;
mod visuals;

use crate::core::assets::{asset_root, asset_server_path};
use crate::core::at;
use crate::core::config::{GameConfig, StepOutcome};
use crate::core::font::game_font;
use crate::core::input::{Actions, GameAction, shift_held};
use crate::core::library::{StepfileId, StepfileLibrary};
use crate::core::note_field::{
    NoteFieldClock, NoteFieldSystems, NoteSpawn, spawn_mine, spawn_note, spawn_receptors,
};
use crate::core::note_skin::ActiveNoteSkin;
use crate::core::scene_flow::SpawnScoped;
use crate::core::settings::{Settings, TimingSettings};
use crate::core::sfx::{PlaySfx, Sfx};
use crate::core::stepfile::NoteKind;
use crate::core::tick_track::render_tick_track;
use crate::core::units::{Beat, Millis, Seconds};
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
    pub outcomes: Vec<StepOutcome>,
    pub max_combo: u32,
    pub holds_ok: u32,
    pub holds_total: u32,
    pub mines_hit: u32,
    pub mines_total: u32,
}

pub struct FilePlayerPlugin;

impl Plugin for FilePlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<JudgmentShown>()
            .add_plugins((
                clock::plugin,
                grading::plugin,
                visuals::plugin,
                background::plugin,
            ))
            .configure_sets(
                Update,
                (
                    (PlaySet::Clock, PlaySet::Judge, PlaySet::Tune, PlaySet::Sync)
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
                    finish_when_complete,
                    handle_cancel.run_if(scene_accepts_input),
                )
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
    Judge,
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
}

const LEAD_IN: Seconds = Seconds(2.0);

#[derive(Resource)]
pub(super) struct PlaySession {
    pub title: String,
    pub notes: Vec<SessionNote>,
    pub mines: Vec<SessionMine>,
    pub judged_count: usize,
    pub expire_cursor: usize,
    pub phase: PlayPhase,
    /// Raw playback time as the audio mixer reports it (queue position).
    pub clock: Seconds,
    pub last_sink_position: Seconds,
    /// Wall-clock time since the tracks were started, for measuring how far
    /// the mixer's queue runs ahead of real time (the audio latency).
    pub wall_since_play: Seconds,
    pub latency_samples: Vec<Seconds>,
    pub combo: u32,
    pub max_combo: u32,
    pub last_note_time: Seconds,
    pub finished: bool,
    /// While enabled, hit errors accumulate and the median of every batch is
    /// folded into the machine offset (AutoSync).
    pub autosync: bool,
    pub autosync_samples: Vec<Seconds>,
}

pub(super) enum PlayPhase {
    LeadIn { remaining: Seconds },
    Playing,
}

impl PlaySession {
    /// What the speakers are playing right now.
    pub fn heard_now(&self, timing: &TimingSettings) -> Seconds {
        self.clock - timing.audio_latency().to_seconds()
    }

    /// The timeline inputs are graded on (shifted by the machine offset).
    pub fn judged_now(&self, timing: &TimingSettings) -> Seconds {
        self.heard_now(timing) + timing.machine_offset.to_seconds()
    }

    /// The timeline arrows are drawn on (shifted by the visual delay).
    pub fn visible_now(&self, timing: &TimingSettings) -> Seconds {
        self.judged_now(timing) - timing.visual_delay.to_seconds()
    }
}

pub(super) struct SessionNote {
    pub time: Seconds,
    pub column: usize,
    pub entity: Entity,
    pub outcome: Option<StepOutcome>,
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
    Hit,
    Avoided,
}

/// Announces a judged note so the judgment/combo displays can react.
#[derive(Message)]
pub(super) struct JudgmentShown {
    pub outcome: StepOutcome,
    pub combo: u32,
}

#[derive(Component, Default, Clone)]
pub(super) struct MusicTrack;

/// The pre-rendered tick track sink, always playing in sync, muted unless
/// tick audio is toggled on.
#[derive(Component, Default, Clone)]
pub(super) struct TickTrack;

#[derive(Component, Default, Clone)]
pub(super) struct JudgmentText;

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

    let mut pending = Vec::new();
    let mut mines = Vec::new();
    let mut last_note_time = Seconds::ZERO;
    for note in &chart.notes {
        if note.column >= 4 {
            continue;
        }
        let time = timing.seconds_at_beat(note.beat);
        last_note_time = last_note_time.max(timing.seconds_at_beat(note.end_beat()));

        if note.kind == NoteKind::Mine {
            let entity = spawn_mine(&mut commands, &assets.skin, time, note.beat, note.column);
            commands
                .entity(entity)
                .insert(DespawnOnExit(GameScene::FilePlayer));
            mines.push(SessionMine {
                time,
                column: note.column,
                entity,
                outcome: None,
            });
            continue;
        }
        if !note.is_steppable() {
            continue;
        }

        let end = match note.kind {
            NoteKind::Hold { end } | NoteKind::Roll { end } => {
                Some((timing.seconds_at_beat(end), end))
            }
            _ => None,
        };
        pending.push(PendingNote {
            time,
            beat: note.beat,
            column: note.column,
            quant: config.recognized_quant(note.quant),
            end,
            roll: matches!(note.kind, NoteKind::Roll { .. }),
        });
    }
    pending.sort_by(|a, b| a.time.0.total_cmp(&b.time.0));

    let mut notes = Vec::new();
    for note in pending {
        let spawned = spawn_note(
            &mut commands,
            &assets.skin,
            &NoteSpawn {
                time: note.time,
                beat: note.beat,
                column: note.column,
                quant: note.quant,
                end: note.end,
            },
        );
        for entity in spawned.entities() {
            commands
                .entity(entity)
                .insert(DespawnOnExit(GameScene::FilePlayer));
        }
        notes.push(SessionNote {
            time: note.time,
            column: note.column,
            entity: spawned.head,
            outcome: None,
            hold: note.end.map(|(end, _)| HoldState {
                end,
                roll: note.roll,
                life: 1.0,
                engaged: false,
                held_now: false,
                result: None,
            }),
        });
    }

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

    let tick_times: Vec<Seconds> = notes.iter().map(|note| note.time).collect();
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

    background::spawn_background(&mut commands, entry, &timing);
    spawn_hud(&mut commands);

    commands.insert_resource(NoteFieldClock {
        visible: -LEAD_IN,
        timing,
        speed: settings.stepfile.note_speed,
    });
    commands.insert_resource(PlaySession {
        title: entry.display_title(),
        notes,
        mines,
        judged_count: 0,
        expire_cursor: 0,
        phase: PlayPhase::LeadIn { remaining: LEAD_IN },
        clock: -LEAD_IN,
        last_sink_position: Seconds(-1.0),
        wall_since_play: Seconds::ZERO,
        latency_samples: Vec::new(),
        combo: 0,
        max_combo: 0,
        last_note_time,
        finished: false,
        autosync: false,
        autosync_samples: Vec::new(),
    });
}

fn exit(mut commands: Commands) {
    commands.remove_resource::<PlaySession>();
    commands.remove_resource::<NoteFieldClock>();
    commands.remove_resource::<SelectedStepfile>();
}

/// A chart note resolved to spawn-ready values, so notes can be sorted by
/// time before the field entities and session records are created together.
struct PendingNote {
    time: Seconds,
    beat: Beat,
    column: usize,
    quant: u32,
    end: Option<(Seconds, Beat)>,
    roll: bool,
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
            JudgmentText
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
    let enabled = !session.autosync;
    session.autosync = enabled;
    session.autosync_samples.clear();
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
    if !session.autosync || session.autosync_samples.len() < AUTOSYNC_SAMPLES {
        return;
    }
    let mut samples = std::mem::take(&mut session.autosync_samples);
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
    let state = (session.autosync, session.autosync_samples.len());
    if *shown == Some(state) {
        return;
    }
    *shown = Some(state);
    let (text, visibility) = &mut *status;
    if session.autosync {
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
    if session.finished || session.judged_count < session.notes.len() {
        return;
    }
    let audio_done = if let Ok(sink) = music.single() {
        sink.empty()
    } else if let Ok(sink) = tick.single() {
        sink.empty()
    } else {
        session.clock.0 > session.last_note_time.0 + 2.0
    };
    // Trailing mines and hold tails can outlive the audio; let them resolve.
    let chart_done = session.clock.0 >= session.last_note_time.0;
    if !audio_done || !chart_done || !matches!(session.phase, PlayPhase::Playing) {
        return;
    }
    session.finished = true;
    let holds: Vec<&HoldState> = session
        .notes
        .iter()
        .filter_map(|note| note.hold.as_ref())
        .collect();
    commands.insert_resource(ScoreResults {
        id: selected.id,
        title: session.title.clone(),
        outcomes: session
            .notes
            .iter()
            .filter_map(|note| note.outcome)
            .collect(),
        max_combo: session.max_combo,
        holds_ok: holds
            .iter()
            .filter(|hold| hold.result == Some(HoldOutcome::Ok))
            .count() as u32,
        holds_total: holds.len() as u32,
        mines_hit: session
            .mines
            .iter()
            .filter(|mine| mine.outcome == Some(MineOutcome::Hit))
            .count() as u32,
        mines_total: session.mines.len() as u32,
    });
    fade.begin(GameScene::Score);
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
