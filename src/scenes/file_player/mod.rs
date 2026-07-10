mod background;
mod clock;
pub mod grade_text;
mod grading;
mod tuning;
mod visuals;

use crate::core::assets::{asset_root, asset_server_path};
use crate::core::audio::{Sound, SoundChannel, SoundPlayer};
use crate::core::config::{GameConfig, RowOutcome};
use crate::core::font::game_font;
use crate::core::health_vial::{HealthVialMaterial, VialSide, spawn_health_vial};
use crate::core::input::{Actions, GameAction, StepDirection};
use crate::core::library::{StepfileEntry, StepfileId, StepfileLibrary};
use crate::core::note_field::{
    FadeOut, InField, LaneView, NoteField, NoteFieldClock, NoteFieldSystems, NoteSpawn, NoteSpeed,
    NoteTail, TARGET_Y, fitted_arrow_size, max_arrow_size, spawn_mine, spawn_note,
    spawn_note_field, spawn_receptors, visible_world_size,
};
use crate::core::note_skin::{ActiveNoteSkin, ActiveNoteSkins};
use crate::core::platform::SoundOptions;
use crate::core::player::PlayerId;
use crate::core::scene_flow::SpawnScoped;
use crate::core::settings::{GradeLayer, MachineSettings, PlayerSettings};
use crate::core::sfx::{PlaySfx, Sfx};
use crate::core::stepfile::{Mine, MusicPlayer, Row, StepfileClock, StepfileTiming};
use crate::core::tick_track::render_tick_track;
use crate::core::units::Seconds;
use crate::core::{OVERLAY_LAYER, SCREEN_SIZE, at};
use crate::scenes::file_select::{FileSelectTarget, SelectedStepfile};
use crate::scenes::{GameScene, SceneFade, scene_accepts_input};
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use strum::IntoEnumIterator;

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
                grade_text::plugin,
                background::plugin,
            ))
            // The engine sets run wherever a session exists — the play stage
            // and the options preview alike; scene-specific systems below add
            // their own FilePlayer gate.
            .configure_sets(
                Update,
                (
                    (
                        PlaySet::Clock,
                        GameplayDrive,
                        PlaySet::Grade,
                        PlaySet::Tune,
                        PlaySet::Sync,
                    )
                        .chain()
                        .before(NoteFieldSystems),
                    PlaySet::Present.after(NoteFieldSystems),
                )
                    .run_if(resource_exists::<PlaySession>.and_then(resource_exists::<PlayTime>)),
            )
            // The real adapter's port drivers: the audio clock fills `PlayTime`
            // (see `clock`) and the keyboard fills `PlayInput`. The mocked
            // adapter fills the same two ports from its own systems; the engine
            // reads only the ports, never the keyboard or the music.
            .add_systems(
                Update,
                wire_keyboard
                    .in_set(GameplayDrive)
                    .run_if(in_state(GameScene::FilePlayer)),
            )
            .add_systems(OnEnter(GameScene::FilePlayer), enter)
            .add_systems(OnExit(GameScene::FilePlayer), exit)
            .add_systems(
                Update,
                refit_stages_to_window
                    .before(NoteFieldSystems)
                    .run_if(in_state(GameScene::FilePlayer)),
            )
            .add_systems(
                Update,
                (
                    fail_drained_stages,
                    finish_when_complete,
                    handle_cancel.run_if(scene_accepts_input),
                )
                    .chain()
                    .in_set(PlaySet::Present)
                    .run_if(in_state(GameScene::FilePlayer)),
            );
    }
}

/// Where a gameplay adapter fills the prefab's ports — [`PlayTime`] (clock)
/// and [`PlayInput`] (input) — before the engine grades. The real adapter
/// wires the keyboard here; the mocked options preview its autoplay. The
/// prefab reads only the ports, so it is agnostic to what drives them.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GameplayDrive;

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

/// The sources everything on stage is materialized from, and the window
/// the stage is fitted to.
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

#[derive(Resource)]
struct PlaySession {
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
struct Stage {
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
struct PlaybackClock {
    pub phase: PlayPhase,
    pub music: StepfileClock,
    /// Wall-clock time since the tracks were started, for measuring how far
    /// the mixer's queue runs ahead of real time (the audio latency).
    pub wall_since_play: Seconds,
    pub latency_samples: Vec<Seconds>,
}

enum PlayPhase {
    LeadIn { remaining: Seconds },
    Playing,
}

/// While enabled, hit errors accumulate and the median of every batch is
/// folded into the machine offset (AutoSync).
#[derive(Default)]
struct AutoSync {
    pub enabled: bool,
    pub samples: Vec<Seconds>,
}

/// The prefab's CLOCK port: the play engine's current moment. A gameplay
/// adapter fills it in [`GameplayDrive`] — the play stage from its audio
/// clock (with tuning), the options preview from the wheel's music. Grading
/// judges against `graded`; the note field draws on `visible`.
#[derive(Resource, Default)]
pub struct PlayTime {
    pub graded: Seconds,
    pub visible: Seconds,
}

/// The prefab's INPUT port: which step panels are held and freshly struck
/// this frame, keyed by their [`GameAction`]. A gameplay adapter fills it in
/// [`GameplayDrive`] — the play stage from the keyboard, the options preview
/// from its deterministic autoplay — and the engine reads only this, never
/// the keyboard directly.
#[derive(Resource, Default)]
pub struct PlayInput {
    held: Vec<GameAction>,
    struck: Vec<GameAction>,
}

impl PlayInput {
    /// Whether the panel is held this frame.
    pub fn held(&self, action: GameAction) -> bool {
        self.held.contains(&action)
    }

    /// Whether the panel went down this frame.
    pub fn struck(&self, action: GameAction) -> bool {
        self.struck.contains(&action)
    }

    /// Clears the frame's input; an adapter refills it every frame.
    pub fn clear(&mut self) {
        self.held.clear();
        self.struck.clear();
    }

    /// Records the panel as held, and freshly struck when `struck`.
    pub fn press(&mut self, action: GameAction, struck: bool) {
        self.held.push(action);
        if struck {
            self.struck.push(action);
        }
    }
}

/// One row of the chart as played: every arrow in it must be stepped, and
/// the whole row resolves into a single outcome (see the grading systems).
struct SessionRow {
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

struct SessionArrow {
    pub column: usize,
    pub entity: Entity,
    /// The banked press: its timing error is locked in silently when the
    /// panel is stepped and only counts once the whole row resolves.
    pub error: Option<Seconds>,
    pub hold: Option<HoldState>,
}

/// Live state of one hold or roll; the life rules live in
/// [`grading::update_holds`].
struct HoldState {
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
pub enum HoldOutcome {
    /// Held to the end.
    Ok,
    /// Dropped, or the head was missed.
    Ng,
}

pub struct SessionMine {
    pub time: Seconds,
    pub column: usize,
    pub entity: Entity,
    pub outcome: Option<MineOutcome>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MineOutcome {
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

/// Tags a HUD element with the stage player it reports on.
#[derive(Component, Clone, Copy, FromTemplate)]
struct ForPlayer(PlayerId);

/// The combo readout, carrying its own bounce animation state, spawned by
/// [`spawn_combo`] under the grade text and driven by the shared combo system.
#[derive(Component, Default, Clone)]
struct ComboText {
    pub bounce: Seconds,
    pub last_combo: u32,
}

#[derive(Component, Default, Clone)]
pub(super) struct OffsetOsd;

#[derive(Component, Default, Clone)]
pub struct AutoSyncText;

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
    let specs: Vec<PackSpec> = charts
        .iter()
        .map(|(player, chart)| PackSpec {
            player: *player,
            columns: chart.columns,
            speed: player_settings[*player].note_speed,
        })
        .collect();
    let layouts = pack_stage_fields(&specs, &config, assets.windows.single().ok());

    let mut stages = Vec::new();
    let mut last_note_time = Seconds::ZERO;
    for ((player, chart), layout) in charts.into_iter().zip(layouts) {
        let grade_layer = player_settings[player].grade_layer;
        let (stage, stage_last) = spawn_field(
            &mut commands,
            PrefabAssets {
                asset_server: &assets.asset_server,
                images: &mut assets.images,
                config: &config,
                skins: &assets.skins,
            },
            &FieldSpec {
                layout: layout.clone(),
                rows: &chart.rows,
                mines: &chart.mines,
                grade_source_layer: STAGE_GRADE_SOURCE_BASE + player_index(player),
                grade_present_layer: (grade_layer == GradeLayer::InFront).then_some(OVERLAY_LAYER),
                max_health: config.player_max_health,
            },
            &timing,
            DespawnOnExit(GameScene::FilePlayer),
        );
        last_note_time = last_note_time.max(stage_last);
        let side = match player {
            PlayerId::P1 => VialSide::Left,
            PlayerId::P2 => VialSide::Right,
        };
        let vial = spawn_health_vial(
            &mut commands,
            &mut assets.vial_materials,
            1.0,
            side,
            config.stage.screen_edge_padding,
        );
        commands
            .entity(vial)
            .insert((ForPlayer(player), DespawnOnExit(GameScene::FilePlayer)));
        stages.push(stage);
    }
    if stages.is_empty() {
        fade.begin(GameScene::FileSelect);
        return;
    }

    spawn_audio_tracks(&mut commands, &mut assets, entry, &stages, &config);
    background::spawn_background(&mut commands, entry, &timing);
    spawn_shared_hud(&mut commands);

    let lead_in = Seconds(config.stage.lead_in_seconds);
    commands.insert_resource(NoteFieldClock {
        visible: -lead_in,
        timing: timing.clone(),
        target_y: TARGET_Y,
    });
    commands.insert_resource(PlaySession {
        title: entry.display_title(),
        stages,
        clock: PlaybackClock {
            phase: PlayPhase::LeadIn { remaining: lead_in },
            music: StepfileClock::start_at(timing, -lead_in),
            wall_since_play: Seconds::ZERO,
            latency_samples: Vec::new(),
        },
        last_note_time,
        finished: false,
        autosync: AutoSync::default(),
    });
    commands.init_resource::<PlayTime>();
    commands.init_resource::<PlayInput>();
}

fn exit(mut commands: Commands, mut music: ResMut<MusicPlayer>) {
    music.stop();
    clear_session(&mut commands);
    commands.remove_resource::<NoteFieldClock>();
    commands.remove_resource::<SelectedStepfile>();
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
            view: LaneView::default(),
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

// ===== The FilePlayer prefab =====
// The reusable stepfile player: a gameplay adapter fills these types and the
// prefab renders + grades the session. The real play stage drives it from the
// audio clock and keyboard (`enter`/`clock`/`grading`); the options preview
// from the wheel music and a scripted autoplay (`file_select::player_options`).

/// One player's field for [`spawn_session`]: what to build and where.
pub struct FieldSpec<'a> {
    pub layout: NoteField,
    pub rows: &'a [Row],
    pub mines: &'a [Mine],
    /// The private layer the grade word renders on offscreen, and the layer
    /// its shader quad (and the combo under it) present on — `None` presents
    /// on the default world layer, behind the arrows.
    pub grade_source_layer: usize,
    pub grade_present_layer: Option<usize>,
    pub max_health: u32,
}

/// A whole session for an adapter to instantiate: one field per spec, timed
/// on `timing`. The clock and input come from the adapter's port drivers (see
/// [`GameplayDrive`]) — a scripted session inserts an autoplay driver, the
/// play stage its audio clock and keyboard. The play stage builds its own
/// [`PlaySession`] inline; this is the entry point the preview uses.
pub struct SessionSpec<'a> {
    pub title: String,
    pub fields: Vec<FieldSpec<'a>>,
    pub timing: StepfileTiming,
}

/// The shared asset handles a field is materialized from, bundled so the
/// spawners stay within the argument limit.
pub struct PrefabAssets<'a> {
    pub asset_server: &'a AssetServer,
    pub images: &'a mut Assets<Image>,
    pub config: &'a GameConfig,
    pub skins: &'a ActiveNoteSkins,
}

/// The grade-text source layer per stage player, clear of the lane and
/// overlay layers.
const STAGE_GRADE_SOURCE_BASE: usize = 20;

/// Builds a session — one field per spec — and inserts the engine's ports
/// ([`PlaySession`], [`PlayTime`], [`PlayInput`]). The caller tags every
/// entity with `scope`, drives the ports each frame, and re-calls this to
/// rebuild.
pub fn spawn_session(
    commands: &mut Commands,
    assets: &mut PrefabAssets,
    spec: SessionSpec,
    scope: impl Bundle + Clone,
) {
    let mut stages = Vec::new();
    for field in &spec.fields {
        let (stage, _) = spawn_field(
            commands,
            PrefabAssets {
                asset_server: assets.asset_server,
                images: assets.images,
                config: assets.config,
                skins: assets.skins,
            },
            field,
            &spec.timing,
            scope.clone(),
        );
        stages.push(stage);
    }
    commands.insert_resource(PlaySession {
        title: spec.title,
        stages,
        clock: idle_clock(spec.timing),
        last_note_time: Seconds::ZERO,
        finished: false,
        autosync: AutoSync::default(),
    });
    commands.init_resource::<PlayTime>();
    commands.init_resource::<PlayInput>();
}

/// Removes the engine ports a [`spawn_session`] inserted.
pub fn clear_session(commands: &mut Commands) {
    commands.remove_resource::<PlaySession>();
    commands.remove_resource::<PlayTime>();
    commands.remove_resource::<PlayInput>();
}

/// A minimal always-playing clock for a manually-driven session.
fn idle_clock(timing: StepfileTiming) -> PlaybackClock {
    PlaybackClock {
        phase: PlayPhase::Playing,
        music: StepfileClock::start_at(timing, Seconds::ZERO),
        wall_since_play: Seconds::ZERO,
        latency_samples: Vec::new(),
    }
}

/// The real adapter's input driver: fills the [`PlayInput`] port from the
/// keyboard. Only wired on the play stage; the preview fills the port itself.
fn wire_keyboard(actions: Actions, input: Option<ResMut<PlayInput>>) {
    let Some(mut input) = input else { return };
    input.clear();
    for player in PlayerId::iter() {
        for column in 0..4 {
            let action = GameAction::step(player, StepDirection::of_column(column));
            if actions.pressed(action) {
                input.press(action, actions.just_pressed(action));
            }
        }
    }
}

/// Builds one player's whole field — field+receptors, notes, grade display,
/// and combo — tagging every entity with `scope`.
fn spawn_field(
    commands: &mut Commands,
    assets: PrefabAssets,
    spec: &FieldSpec,
    timing: &StepfileTiming,
    scope: impl Bundle + Clone,
) -> (Stage, Seconds) {
    let player = spec.layout.player;
    let origin_x = spec.layout.origin_x;
    let skin = assets.skins.get(player);
    let field = spawn_note_field(commands, spec.layout.clone());
    commands.entity(field).insert(scope.clone());
    for entity in spawn_receptors(commands, skin, field, &spec.layout) {
        commands.entity(entity).insert(scope.clone());
    }
    let (rows, mines, last_note_time) = spawn_chart(
        commands,
        assets.asset_server,
        &StageChart {
            field,
            layout: &spec.layout,
            skin,
            rows: spec.rows,
            mines: spec.mines,
            timing,
        },
        assets.config,
        scope.clone(),
    );
    grade_text::spawn_display(
        commands,
        assets.images,
        assets.asset_server,
        grade_text::GradeSpawn {
            player,
            origin_x,
            source_layer: spec.grade_source_layer,
            present_layer: spec.grade_present_layer,
        },
        scope.clone(),
    );
    spawn_combo(commands, player, origin_x, spec.grade_present_layer, scope);
    let stage = Stage {
        player,
        field,
        rows,
        mines,
        graded_count: 0,
        expire_cursor: 0,
        combo: 0,
        max_combo: 0,
        health: spec.max_health,
        failed: false,
    };
    (stage, last_note_time)
}

/// One stage's field being filled with notes: where the notes go and what
/// they are.
struct StageChart<'a> {
    pub field: Entity,
    pub layout: &'a NoteField,
    pub skin: &'a ActiveNoteSkin,
    pub rows: &'a [Row],
    pub mines: &'a [Mine],
    pub timing: &'a StepfileTiming,
}

/// Spawns every note and mine into the field, tagged with `scope`, and
/// returns the session records tracking them plus the time the chart is over.
fn spawn_chart(
    commands: &mut Commands,
    asset_server: &AssetServer,
    stage: &StageChart,
    config: &GameConfig,
    scope: impl Bundle + Clone,
) -> (Vec<SessionRow>, Vec<SessionMine>, Seconds) {
    let timing = stage.timing;
    let mut session_mines = Vec::new();
    let mut last_note_time = Seconds::ZERO;
    for mine in stage.mines {
        let time = timing.seconds_at_beat(mine.beat);
        last_note_time = last_note_time.max(time);
        let entity = spawn_mine(
            commands,
            stage.skin,
            stage.field,
            stage.layout,
            time,
            mine.beat,
            mine.column,
        );
        commands.entity(entity).insert(scope.clone());
        session_mines.push(SessionMine {
            time,
            column: mine.column,
            entity,
            outcome: None,
        });
    }

    let mut session_rows = Vec::new();
    for row in stage.rows {
        let time = timing.seconds_at_beat(row.beat);
        let quant = config.recognized_quant(row.quant);
        let mut arrows = Vec::new();
        for arrow in &row.arrows {
            let tail = arrow.tail.map(|tail| NoteTail {
                time: timing.seconds_at_beat(tail.end),
                beat: tail.end,
                roll: tail.roll,
            });
            last_note_time =
                last_note_time.max(tail.as_ref().map(|tail| tail.time).unwrap_or(time));
            let spawned = spawn_note(
                commands,
                asset_server,
                stage.skin,
                stage.field,
                stage.layout,
                &NoteSpawn {
                    time,
                    beat: row.beat,
                    column: arrow.column,
                    quant,
                    tail,
                },
            );
            for entity in spawned.entities() {
                commands.entity(entity).insert(scope.clone());
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
        session_rows.push(SessionRow {
            time,
            outcome: None,
            arrows,
        });
    }
    // Warps and stops can reorder wall-clock times relative to beats; the
    // expiry cursor needs time order.
    session_rows.sort_by(|a, b| a.time.0.total_cmp(&b.time.0));
    (session_rows, session_mines, last_note_time)
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
                        volume: assets.machine.volume.sfx_gain(),
                        ..default()
                    },
                });
        }
        Err(error) => warn!("could not render tick track: {error}"),
    }
}

/// The per-stage combo readout, tracked under the grade text (see
/// [`visuals::update_combo_texts`]) and sharing its present layer. The grade
/// text itself is a shader rig spawned by [`grade_text::spawn_display`].
fn spawn_combo(
    commands: &mut Commands,
    player: PlayerId,
    origin_x: f32,
    present_layer: Option<usize>,
    scope: impl Bundle,
) {
    let combo = commands
        .spawn_scene(bsn! {
            ComboText
            ForPlayer({player})
            game_font(44.0)
            Text2d("")
            TextColor(Color::WHITE)
            at(origin_x, 0.0, 5.0)
            Visibility::Hidden
        })
        .insert(scope)
        .id();
    if let Some(layer) = present_layer {
        commands.entity(combo).insert(RenderLayers::layer(layer));
    }
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
        // The whole side shuts down: notes, mines, and receptors shrink
        // and fade away.
        for (entity, in_field) in &staged {
            if in_field.0 == stage.field {
                commands
                    .entity(entity)
                    .insert(FadeOut::growing(FAIL_FADE_SECONDS, -1.0));
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
