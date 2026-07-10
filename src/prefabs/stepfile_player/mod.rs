//! The stepfile player: the reusable play engine that materializes note
//! fields from chart data, scrolls and animates them in the player's skin
//! and perspective, grades every row, and pops grade words and combos —
//! the same machinery whether it fills a live play stage or an embedded
//! autoplayed preview.
//!
//! An adapter instantiates it with [`stepfile_player_prefab`] and then
//! drives the two ports every frame, in [`GameplayDrive`]: [`PlayTime`]
//! (the clock) and [`PlayInput`] (the panels). The engine reads only the
//! ports — never a keyboard, never a music player — and reports back
//! through messages ([`RowGraded`], [`PressBanked`], [`StageFailed`]) and
//! the [`PlaySession`] state.

pub mod grade_text;
pub mod note_field;
pub mod note_skin;

mod grading;

use self::grade_text::{COMBO_GAP, GradeArea, GradeSpawn, GradeTextPlugin, grade_y};
use self::note_field::{
    HoldVisual, HoldVisualState, InColumn, InField, NoteField, NoteFieldClock, NoteFieldPlugin,
    NoteFieldSystems, NoteSpawn, NoteTail, Receptor, spawn_mine, spawn_note, spawn_note_field,
    spawn_receptors,
};
use self::note_skin::{ActiveNoteSkin, ActiveNoteSkins, NoteSkinPlugin};
use crate::core::at;
use crate::core::config::{GameConfig, RowOutcome};
use crate::core::font::game_font;
use crate::core::input::GameAction;
use crate::core::player::PlayerId;
use crate::core::settings::PlayerSettings;
use crate::core::stepfile::{Mine, Row, StepfileTiming};
use crate::core::units::Seconds;
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::system::EntityCommands;
use bevy::prelude::*;
use std::sync::Arc;

pub struct StepfilePlayerPrefabOptions<'a, Scope: Bundle + Clone> {
    /// One field per active player; a doubles session is one wide field.
    pub fields: Vec<FieldSpec<'a>>,
    pub timing: StepfileTiming,
    /// Tagged onto every entity the session spawns — at instantiation and
    /// mid-play (flashes, popups) — so the caller owns the teardown.
    pub scope: Scope,
}

/// Builds a session — fields, receptors, notes, grade displays, combos —
/// and inserts the engine's state and ports ([`PlaySession`], [`PlayTime`],
/// [`PlayInput`]). The caller drives the ports each frame (see
/// [`GameplayDrive`]), owns the [`NoteFieldClock`] and [`GradeArea`], and
/// re-calls this to rebuild. Returns the moment the last note (or hold
/// tail) is over, for owners that pace a session's end.
pub fn stepfile_player_prefab(
    opt: StepfilePlayerPrefabOptions<impl Bundle + Clone>,
    commands: &mut Commands,
    assets: &mut StepfilePlayerAssets,
) -> Seconds {
    let scope = opt.scope;
    let mut stages = Vec::new();
    let mut last_note_time = Seconds::ZERO;
    for field in &opt.fields {
        let (stage, stage_last) = spawn_field(commands, assets, field, &opt.timing, scope.clone());
        last_note_time = last_note_time.max(stage_last);
        stages.push(stage);
    }
    commands.insert_resource(PlaySession {
        stages,
        scope: Arc::new(move |entity: &mut EntityCommands| {
            entity.insert(scope.clone());
        }),
    });
    commands.init_resource::<PlayTime>();
    commands.init_resource::<PlayInput>();
    last_note_time
}

/// Removes the engine state and ports a [`stepfile_player_prefab`] call
/// inserted; the entities die with the caller's scope.
pub fn clear_session(commands: &mut Commands) {
    commands.remove_resource::<PlaySession>();
    commands.remove_resource::<PlayTime>();
    commands.remove_resource::<PlayInput>();
}

/// One player's field: what to build and where to render its pieces.
pub struct FieldSpec<'a> {
    pub layout: NoteField,
    pub rows: &'a [Row],
    pub mines: &'a [Mine],
    /// The private layer the grade word renders on offscreen.
    pub grade_source_layer: usize,
    /// The 2D layer the grade word's shader quad (and the combo under it)
    /// present on; `None` presents on the default world layer, behind the
    /// arrows.
    pub grade_present_layer: Option<usize>,
    /// The 2D layer hold Ok/NG popups draw on; `None` for the default
    /// world layer.
    pub popup_layer: Option<usize>,
    /// The stage's health capacity and starting health.
    pub max_health: u32,
}

/// The shared asset handles a session is materialized from, bundled so the
/// spawners stay within the argument limit.
pub struct StepfilePlayerAssets<'a> {
    pub asset_server: &'a AssetServer,
    pub images: &'a mut Assets<Image>,
    pub config: &'a GameConfig,
    pub skins: &'a ActiveNoteSkins,
}

/// Where a gameplay adapter fills the ports — [`PlayTime`] and
/// [`PlayInput`] — before the engine grades. A live stage wires its audio
/// clock and keyboard here; a scripted preview its music and autoplay.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GameplayDrive;

/// The engine's frame phases around the note fields: `Grade` and `Sync`
/// feed them and run before [`NoteFieldSystems`]; `Present` reacts to the
/// graded frame after it. Owners hang their own session systems off these.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlaySet {
    Grade,
    Sync,
    Present,
}

/// The engine's CLOCK port: the session's current moment. An adapter fills
/// it in [`GameplayDrive`]. Grading judges against `graded`; the note
/// fields draw on `visible`.
#[derive(Resource, Default)]
pub struct PlayTime {
    pub graded: Seconds,
    pub visible: Seconds,
}

/// The engine's INPUT port: which step panels are held and freshly struck
/// this frame, keyed by their [`GameAction`]. An adapter fills it in
/// [`GameplayDrive`]; the engine reads only this, never a device.
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

/// Announces a stage's graded row so grade and combo displays can react.
#[derive(Message)]
pub struct RowGraded {
    pub player: PlayerId,
    pub outcome: RowOutcome,
    pub combo: u32,
}

/// A press banked into an arrow, with its signed timing error (positive =
/// early) — the raw sample stream timing tools feed on.
#[derive(Message)]
pub struct PressBanked {
    pub error: Seconds,
}

/// A stage drained to zero health and shut down; its field is already
/// fading. The session's owner decides what a failure means.
#[derive(Message)]
pub struct StageFailed {
    pub player: PlayerId,
}

/// The live session the engine grades: one [`Stage`] per field. Owners
/// read it for presentation and flow; only the engine mutates it.
#[derive(Resource)]
pub struct PlaySession {
    stages: Vec<Stage>,
    /// Applies the instantiation's scope to entities the engine spawns
    /// mid-session (flashes, explosions, popups).
    scope: SessionScope,
}

type SessionScope = Arc<dyn Fn(&mut EntityCommands) + Send + Sync>;

impl PlaySession {
    pub fn stages(&self) -> &[Stage] {
        &self.stages
    }

    /// Whether every stage has either failed or graded its whole chart.
    pub fn all_settled(&self) -> bool {
        self.stages
            .iter()
            .all(|stage| stage.failed || stage.complete())
    }

    pub fn all_failed(&self) -> bool {
        self.stages.iter().all(|stage| stage.failed)
    }

    /// Every stage's results, in field order.
    pub fn results(&self) -> Vec<StageResults> {
        self.stages.iter().map(Stage::results).collect()
    }
}

/// One player's run through their chart: their notes, their grading
/// cursors, their combo and health. The field's geometry and input mapping
/// live on the [`NoteField`] component of its entity.
pub struct Stage {
    pub player: PlayerId,
    field: Entity,
    rows: Vec<SessionRow>,
    mines: Vec<SessionMine>,
    graded_count: usize,
    expire_cursor: usize,
    combo: u32,
    max_combo: u32,
    health: u32,
    max_health: u32,
    failed: bool,
    popup_layer: Option<usize>,
}

impl Stage {
    /// Every row has been graded.
    pub fn complete(&self) -> bool {
        self.graded_count >= self.rows.len()
    }

    pub fn failed(&self) -> bool {
        self.failed
    }

    /// Current health as a `0..=1` fraction of the stage's capacity.
    pub fn health_fraction(&self) -> f32 {
        self.health as f32 / self.max_health as f32
    }

    pub fn results(&self) -> StageResults {
        let holds: Vec<&HoldState> = self
            .rows
            .iter()
            .flat_map(|row| &row.arrows)
            .filter_map(|arrow| arrow.hold.as_ref())
            .collect();
        StageResults {
            player: self.player,
            failed: self.failed,
            outcomes: self.rows.iter().filter_map(|row| row.outcome).collect(),
            rows_total: self.rows.len() as u32,
            max_combo: self.max_combo,
            holds_ok: holds
                .iter()
                .filter(|hold| hold.result == Some(HoldOutcome::Ok))
                .count() as u32,
            holds_ng: holds
                .iter()
                .filter(|hold| hold.result == Some(HoldOutcome::Ng))
                .count() as u32,
            holds_total: holds.len() as u32,
            mines_exploded: self
                .mines
                .iter()
                .filter(|mine| mine.outcome == Some(MineOutcome::Exploded))
                .count() as u32,
            mines_total: self.mines.len() as u32,
        }
    }
}

/// One stage's complete run, derived from the raw outcomes. Grades are
/// derived from the outcomes by whoever displays them.
#[derive(Debug, Clone)]
pub struct StageResults {
    pub player: PlayerId,
    /// The run drained to zero health before the chart ended.
    pub failed: bool,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldOutcome {
    /// Held to the end.
    Ok,
    /// Dropped, or the head was missed.
    Ng,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MineOutcome {
    Exploded,
    Avoided,
}

/// Tags a session HUD element (grade text, combo, a caller's own widgets)
/// with the stage player it reports on.
#[derive(Component, Clone, Copy, FromTemplate)]
pub struct ForPlayer(pub PlayerId);

/// The engine's systems and asset plugins. Requires `GameConfig` and the
/// settings resources to already be inserted (skins load from them).
pub struct StepfilePlayerPlugin;

impl Plugin for StepfilePlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((NoteSkinPlugin, NoteFieldPlugin, GradeTextPlugin))
            .add_message::<RowGraded>()
            .add_message::<PressBanked>()
            .add_message::<StageFailed>()
            .configure_sets(
                Update,
                (
                    (GameplayDrive, PlaySet::Grade, PlaySet::Sync)
                        .chain()
                        .before(NoteFieldSystems),
                    PlaySet::Present.after(NoteFieldSystems),
                )
                    .run_if(resource_exists::<PlaySession>.and_then(resource_exists::<PlayTime>)),
            )
            .add_plugins((grading::plugin, grade_text::plugin))
            .add_systems(Update, sync_note_field.in_set(PlaySet::Sync))
            .add_systems(Update, update_combo_texts.in_set(PlaySet::Present));
    }
}

/// One row of the chart as played: every arrow in it must be stepped, and
/// the whole row resolves into a single outcome (see the grading systems).
struct SessionRow {
    time: Seconds,
    outcome: Option<RowOutcome>,
    /// Simultaneous arrows; two or more make the row a jump.
    arrows: Vec<SessionArrow>,
}

impl SessionRow {
    /// Rows resolve once every arrow has a banked press.
    fn complete(&self) -> bool {
        self.arrows.iter().all(|arrow| arrow.error.is_some())
    }
}

struct SessionArrow {
    column: usize,
    entity: Entity,
    /// The banked press: its timing error is locked in silently when the
    /// panel is stepped and only counts once the whole row resolves.
    error: Option<Seconds>,
    hold: Option<HoldState>,
}

/// Live state of one hold or roll; the life rules live in the grading
/// systems.
struct HoldState {
    end: Seconds,
    roll: bool,
    life: f32,
    /// The head was stepped on, activating the hold.
    engaged: bool,
    /// Whether the panel is currently satisfied (held, for holds).
    held_now: bool,
    result: Option<HoldOutcome>,
}

struct SessionMine {
    time: Seconds,
    column: usize,
    entity: Entity,
    outcome: Option<MineOutcome>,
}

/// Builds one player's whole field — field+receptors, notes, grade display,
/// and combo — tagging every entity with `scope`.
fn spawn_field(
    commands: &mut Commands,
    assets: &mut StepfilePlayerAssets,
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
        GradeSpawn {
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
        max_health: spec.max_health,
        failed: false,
        popup_layer: spec.popup_layer,
    };
    (stage, last_note_time)
}

/// One stage's field being filled with notes: where the notes go and what
/// they are.
struct StageChart<'a> {
    field: Entity,
    layout: &'a NoteField,
    skin: &'a ActiveNoteSkin,
    rows: &'a [Row],
    mines: &'a [Mine],
    timing: &'a StepfileTiming,
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

/// The combo readout, carrying its own bounce animation state, tracked
/// under the grade text and driven by [`update_combo_texts`].
#[derive(Component, Default, Clone)]
struct ComboText {
    bounce: Seconds,
    last_combo: u32,
}

/// The per-stage combo readout, sharing the grade text's present layer.
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

/// Pushes the session's state into the note fields: the drawn timeline,
/// each field's receptors' pressed panels, and every hold's render state.
/// Runs after grading and before the fields' animation systems.
fn sync_note_field(
    input: Res<PlayInput>,
    session: Res<PlaySession>,
    play_time: Res<PlayTime>,
    mut clock: ResMut<NoteFieldClock>,
    fields: Query<&NoteField>,
    mut receptors: Query<(&mut Receptor, &InColumn, &InField)>,
    mut holds: Query<&mut HoldVisual>,
) {
    clock.visible = play_time.visible;

    for (mut receptor, anchor, in_field) in &mut receptors {
        let Ok(field) = fields.get(in_field.0) else {
            continue;
        };
        let held = input.held(field.step_action(anchor.column));
        if receptor.held != held {
            receptor.held = held;
        }
    }

    for stage in &session.stages {
        for arrow in stage.rows.iter().flat_map(|row| &row.arrows) {
            let Some(hold) = &arrow.hold else { continue };
            let state = match (hold.engaged, hold.result) {
                (_, Some(HoldOutcome::Ok)) => HoldVisualState::Ok,
                (_, Some(HoldOutcome::Ng)) => HoldVisualState::Dropped,
                (false, None) => HoldVisualState::Pending,
                (true, None) if hold.held_now => HoldVisualState::Held,
                (true, None) => HoldVisualState::Released,
            };
            if let Ok(mut visual) = holds.get_mut(arrow.entity)
                && visual.state != state
            {
                visual.state = state;
            }
        }
    }
}

const COMBO_BOUNCE: Seconds = Seconds(0.18);

/// Runs every combo readout: refreshed and bounced by its player's graded
/// rows, tracking under the grade word at the height the [`GradeArea`]
/// maps the player's grade-position option to.
fn update_combo_texts(
    time: Res<Time>,
    settings: Res<PlayerSettings>,
    area: Option<Res<GradeArea>>,
    mut graded: MessageReader<RowGraded>,
    mut labels: Query<(
        &ForPlayer,
        &mut ComboText,
        &mut Text2d,
        &mut Transform,
        &mut Visibility,
    )>,
) {
    for message in graded.read() {
        for (owner, mut combo, mut text, _, mut visibility) in &mut labels {
            if owner.0 != message.player {
                continue;
            }
            if message.combo > combo.last_combo {
                combo.bounce = COMBO_BOUNCE;
            }
            combo.last_combo = message.combo;
            if message.combo == 0 {
                *visibility = Visibility::Hidden;
            } else {
                *visibility = Visibility::Visible;
                text.0 = format!("{} combo", message.combo);
            }
        }
    }
    for (owner, mut combo, _, mut transform, _) in &mut labels {
        combo.bounce = (combo.bounce - Seconds(time.delta_secs_f64())).max(Seconds::ZERO);
        let scale = 1.0 + 0.22 * (combo.bounce / COMBO_BOUNCE) as f32;
        if transform.scale.x != scale {
            transform.scale = Vec3::splat(scale);
        }
        if let Some(area) = &area {
            let y = grade_y(area, settings[owner.0].grade_position) - COMBO_GAP;
            if transform.translation.y != y {
                transform.translation.y = y;
            }
        }
    }
}
