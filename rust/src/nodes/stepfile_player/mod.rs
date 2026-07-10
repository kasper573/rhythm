//! The stepfile player: the reusable play engine that materializes note
//! fields from chart data, scrolls and animates them in the player's skin
//! and perspective, grades every row, and pops grade words and combos —
//! the same machinery whether it fills a live play stage or an embedded
//! autoplayed preview.
//!
//! An adapter instantiates it and then drives the two ports every frame,
//! before the node's own process runs (the adapter is its parent, so tree
//! order guarantees it): [`set_time`](StepfilePlayer::set_time) (the clock)
//! and the input port ([`clear_input`](StepfilePlayer::clear_input) +
//! [`press`](StepfilePlayer::press)). The engine reads only the ports —
//! never a keyboard, never a music player — and reports back through
//! signals (`press_banked`, `stage_failed`) and its session state.
//!
//! Everything the session draws is contained in this node's own sandwich:
//! grade words behind the lanes, one transparent lane viewport per field,
//! and popups (plus in-front grade words) on top. Freeing the node frees
//! the whole session.

pub mod grade_text;
pub mod note_field;
pub mod note_skin;

mod grading;

use self::grade_text::{COMBO_GAP, GradeArea, GradeDisplay, grade_y};
use self::note_field::{
    FieldClock, FieldLayout, HoldVisualState, MineIndex, NoteFieldRig, NoteIndex, NoteSpawn,
    NoteTail,
};
use self::note_skin::load_note_skin;
use crate::core::config::{RowOutcome, config};
use crate::core::font::{TextPivot, label, place_label};
use crate::core::input::GameAction;
use crate::core::player::PlayerId;
use crate::core::settings::{GradeLayer, Settings};
use crate::core::stepfile::{Mine, Row, StepfileTiming};
use crate::core::units::Seconds;
use godot::classes::control::LayoutPreset;
use godot::classes::{Control, IControl, Label, Node2D};
use godot::prelude::*;

pub struct StepfilePlayerOptions {
    /// One field per active player; a doubles session is one wide field.
    pub fields: Vec<FieldSpec>,
    pub timing: StepfileTiming,
    /// The design canvas the stage fills: the visible rect for the play
    /// stage, the preview band for the options modal.
    pub canvas: Vector2,
}

/// One player's field: what to build and how much health backs it.
pub struct FieldSpec {
    pub layout: FieldLayout,
    pub rows: Vec<Row>,
    pub mines: Vec<Mine>,
    /// The stage's health capacity and starting health.
    pub max_health: u32,
}

/// The engine's INPUT port: which step panels are held and freshly struck
/// this frame, keyed by their [`GameAction`]. An adapter fills it every
/// frame; the engine reads only this, never a device.
#[derive(Default)]
struct PlayInput {
    held: Vec<GameAction>,
    struck: Vec<GameAction>,
}

impl PlayInput {
    fn held(&self, action: GameAction) -> bool {
        self.held.contains(&action)
    }

    fn struck(&self, action: GameAction) -> bool {
        self.struck.contains(&action)
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

/// One player's run through their chart: their notes, their grading
/// cursors, their combo and health. The field's geometry and input mapping
/// live on its rig's [`FieldLayout`].
struct Stage {
    player: PlayerId,
    rows: Vec<SessionRow>,
    mines: Vec<SessionMine>,
    graded_count: usize,
    expire_cursor: usize,
    combo: u32,
    max_combo: u32,
    health: u32,
    max_health: u32,
    failed: bool,
}

impl Stage {
    /// Every row has been graded.
    fn complete(&self) -> bool {
        self.graded_count >= self.rows.len()
    }

    /// Current health as a `0..=1` fraction of the stage's capacity.
    fn health_fraction(&self) -> f32 {
        self.health as f32 / self.max_health as f32
    }

    fn results(&self) -> StageResults {
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

/// One row of the chart as played: every arrow in it must be stepped, and
/// the whole row resolves into a single outcome (see the grading methods).
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
    note: NoteIndex,
    /// The banked press: its timing error is locked in silently when the
    /// panel is stepped and only counts once the whole row resolves.
    error: Option<Seconds>,
    hold: Option<HoldState>,
}

/// Live state of one hold or roll; the life rules live in the grading
/// methods.
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
    mine: MineIndex,
    outcome: Option<MineOutcome>,
}

/// The combo readout under a player's grade word, with its bounce.
struct ComboDisplay {
    player: PlayerId,
    origin_x: f32,
    label: Gd<Label>,
    bounce: Seconds,
    last_combo: u32,
}

/// One 2D transient (hold popups): grows while fading, then frees.
struct Fading2d {
    node: Gd<Label>,
    remaining: f32,
    total: f32,
    growth: f32,
    base_scale: Vector2,
}

/// What one grading pass reports outward, applied after the pass so the
/// mutable session borrow is already released.
enum StageEvent {
    Graded {
        player: PlayerId,
        outcome: RowOutcome,
        combo: u32,
    },
    PressBanked {
        error: Seconds,
    },
    Failed {
        player: PlayerId,
    },
}

const COMBO_BOUNCE: Seconds = Seconds(0.18);
const HOLD_POPUP_SECONDS: f32 = 0.6;
const FAIL_FADE_SECONDS: f32 = 0.8;

#[derive(GodotClass)]
#[class(base=Control)]
pub struct StepfilePlayer {
    timing: StepfileTiming,
    canvas: Vector2,
    pixel_scale: f32,
    target_y: f32,
    grade_area: GradeArea,
    graded_now: Seconds,
    visible_now: Seconds,
    input: PlayInput,
    stages: Vec<Stage>,
    rigs: Vec<NoteFieldRig>,
    grades: Vec<GradeDisplay>,
    combos: Vec<ComboDisplay>,
    fades: Vec<Fading2d>,
    behind: Option<Gd<Node2D>>,
    overlay: Option<Gd<Node2D>>,
    last_note_time: Seconds,
    base: Base<Control>,
}

#[godot_api]
impl StepfilePlayer {
    /// A press banked into an arrow, with its signed timing error in
    /// seconds (positive = early) — the raw sample stream timing tools
    /// feed on.
    #[signal]
    pub fn press_banked(error: f64);

    /// A stage drained to zero health and shut down; its field is already
    /// fading. `player` indexes [`PlayerId`] in declaration order. The
    /// session's owner decides what a failure means.
    #[signal]
    pub fn stage_failed(player: i64);

    /// Builds a session — fields, receptors, notes, grade displays, and
    /// combos. The caller adds the node where the stage belongs and drives
    /// the ports each frame.
    pub fn instantiate(opt: StepfilePlayerOptions) -> Gd<StepfilePlayer> {
        let mut player = StepfilePlayer::new_alloc();
        player.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        player.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);

        let mut behind = Node2D::new_alloc();
        player.add_child(&behind);
        {
            let mut bound = player.bind_mut();
            bound.timing = opt.timing.clone();
            bound.canvas = opt.canvas;
            for spec in opt.fields {
                bound.build_field(spec);
            }
        }
        let mut overlay = Node2D::new_alloc();
        player.add_child(&overlay);

        let settings = Settings::singleton();
        let mut bound = player.bind_mut();
        for index in 0..bound.rigs.len() {
            let player_id = bound.rigs[index].layout.player;
            let origin_x = bound.rigs[index].layout.origin_x;
            let layer = match settings.bind().player(player_id).grade_layer {
                GradeLayer::Behind => &mut behind,
                GradeLayer::InFront => &mut overlay,
            };
            bound
                .grades
                .push(GradeDisplay::new(layer, player_id, origin_x));
            let mut combo = label("", 44.0, Color::WHITE);
            combo.set_visible(false);
            layer.add_child(&combo);
            bound.combos.push(ComboDisplay {
                player: player_id,
                origin_x,
                label: combo,
                bounce: Seconds::ZERO,
                last_combo: 0,
            });
        }
        bound.behind = Some(behind);
        bound.overlay = Some(overlay);
        drop(bound);
        player
    }

    /// The moment the last note (or hold tail) is over, for owners that
    /// pace a session's end.
    pub fn last_note_time(&self) -> Seconds {
        self.last_note_time
    }

    /// The engine's CLOCK port: grading judges against `graded`; the note
    /// fields draw on `visible`.
    pub fn set_time(&mut self, graded: Seconds, visible: Seconds) {
        self.graded_now = graded;
        self.visible_now = visible;
    }

    /// Clears the frame's input; the adapter refills it every frame.
    pub fn clear_input(&mut self) {
        self.input.held.clear();
        self.input.struck.clear();
    }

    /// Records the panel as held, and freshly struck when `struck`.
    pub fn press(&mut self, action: GameAction, struck: bool) {
        self.input.held.push(action);
        if struck {
            self.input.struck.push(action);
        }
    }

    /// Anchors the receptor row (canvas-centered y-up), for owners that
    /// track the window's top edge.
    pub fn set_target_y(&mut self, target_y: f32) {
        self.target_y = target_y;
    }

    /// The canvas Y band grade words map their height option to.
    pub fn set_grade_area(&mut self, area: GradeArea) {
        self.grade_area = area;
    }

    /// The design canvas and its pixel density, re-applied on window
    /// changes so lanes re-render at native resolution.
    pub fn set_canvas(&mut self, canvas: Vector2, pixel_scale: f32) {
        self.canvas = canvas;
        self.pixel_scale = pixel_scale;
        for rig in &mut self.rigs {
            rig.set_canvas(canvas, pixel_scale);
        }
    }

    /// Re-sizes and re-places the fields — the window rescaling the arrow
    /// pixel budget — without respawning them. Layouts arrive in field
    /// order.
    pub fn refit(&mut self, layouts: Vec<FieldLayout>) {
        for (rig, layout) in self.rigs.iter_mut().zip(layouts) {
            rig.layout = layout;
        }
        for index in 0..self.rigs.len() {
            let origin_x = self.rigs[index].layout.origin_x;
            if let Some(grade) = self.grades.get_mut(index) {
                grade.set_origin_x(origin_x);
            }
            if let Some(combo) = self.combos.get_mut(index) {
                combo.origin_x = origin_x;
            }
        }
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

    /// The active players, in field order.
    pub fn players(&self) -> Vec<PlayerId> {
        self.stages.iter().map(|stage| stage.player).collect()
    }

    /// The visible beat, through the session's timing.
    pub fn visible_beat(&self) -> crate::core::units::Beat {
        self.timing.beat_at_seconds(self.visible_now)
    }

    /// Every field's current layout, in field order.
    pub fn field_layouts(&self) -> Vec<FieldLayout> {
        self.rigs.iter().map(|rig| rig.layout.clone()).collect()
    }

    pub fn health_fraction(&self, player: PlayerId) -> Option<f32> {
        self.stages
            .iter()
            .find(|stage| stage.player == player)
            .map(Stage::health_fraction)
    }

    /// Every stage's results, in field order.
    pub fn results(&self) -> Vec<StageResults> {
        self.stages.iter().map(Stage::results).collect()
    }

    fn build_field(&mut self, spec: FieldSpec) {
        let settings = Settings::singleton();
        let player = spec.layout.player;
        let skin = load_note_skin(&settings.bind().player(player).note_skin);
        let perspective = settings.bind().player(player).perspective;
        let lane_camera = &config().lane_camera;
        let mut host = self.base().clone().cast::<Control>();
        let mut rig = NoteFieldRig::build(
            &mut host,
            spec.layout,
            skin,
            perspective,
            lane_camera.fov_degrees,
            lane_camera.tilt_degrees,
            self.canvas,
        );

        let timing = self.timing.clone();
        let mut session_mines = Vec::new();
        for mine in &spec.mines {
            let time = timing.seconds_at_beat(mine.beat);
            self.last_note_time = self.last_note_time.max(time);
            let index = rig.spawn_mine(time, mine.beat, mine.column);
            session_mines.push(SessionMine {
                time,
                column: mine.column,
                mine: index,
                outcome: None,
            });
        }

        let mut session_rows = Vec::new();
        for row in &spec.rows {
            let time = timing.seconds_at_beat(row.beat);
            let quant = config().recognized_quant(row.quant);
            let mut arrows = Vec::new();
            for arrow in &row.arrows {
                let tail = arrow.tail.map(|tail| NoteTail {
                    time: timing.seconds_at_beat(tail.end),
                    beat: tail.end,
                    roll: tail.roll,
                });
                self.last_note_time = self
                    .last_note_time
                    .max(tail.as_ref().map(|tail| tail.time).unwrap_or(time));
                let note = rig.spawn_note(&NoteSpawn {
                    time,
                    beat: row.beat,
                    column: arrow.column,
                    quant,
                    tail,
                });
                arrows.push(SessionArrow {
                    column: arrow.column,
                    note,
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
        // Warps and stops can reorder wall-clock times relative to beats;
        // the expiry cursor needs time order.
        session_rows.sort_by(|a, b| a.time.0.total_cmp(&b.time.0));

        self.rigs.push(rig);
        self.stages.push(Stage {
            player,
            rows: session_rows,
            mines: session_mines,
            graded_count: 0,
            expire_cursor: 0,
            combo: 0,
            max_combo: 0,
            health: spec.max_health,
            max_health: spec.max_health,
            failed: false,
        });
    }

    /// Spawns a stage's Ok/NG popup above its receptors.
    fn spawn_hold_popup(&mut self, x: f32, outcome: HoldOutcome) {
        let def = match outcome {
            HoldOutcome::Ok => &config().grading.fixed.ok,
            HoldOutcome::Ng => &config().grading.fixed.ng,
        };
        let mut popup = label(&def.name, 30.0, def.color);
        let Some(overlay) = &mut self.overlay else {
            return;
        };
        overlay.add_child(&popup);
        let y = self.target_y - 54.0;
        place_label(&mut popup, Vector2::new(x, -y), TextPivot::CENTER);
        let size = popup.get_size();
        popup.set_pivot_offset(size / 2.0);
        self.fades.push(Fading2d {
            node: popup,
            remaining: HOLD_POPUP_SECONDS,
            total: HOLD_POPUP_SECONDS,
            growth: 0.25,
            base_scale: Vector2::ONE,
        });
    }

    /// Pushes the session's state into the note fields: each field's
    /// receptors' pressed panels and every hold's render state.
    fn sync_fields(&mut self) {
        for (stage, rig) in self.stages.iter().zip(&mut self.rigs) {
            for column in 0..rig.layout.columns {
                let held = self.input.held(rig.layout.step_action(column));
                rig.set_receptor_held(column, held);
            }
            for arrow in stage.rows.iter().flat_map(|row| &row.arrows) {
                let Some(hold) = &arrow.hold else { continue };
                let state = match (hold.engaged, hold.result) {
                    (_, Some(HoldOutcome::Ok)) => HoldVisualState::Ok,
                    (_, Some(HoldOutcome::Ng)) => HoldVisualState::Dropped,
                    (false, None) => HoldVisualState::Pending,
                    (true, None) if hold.held_now => HoldVisualState::Held,
                    (true, None) => HoldVisualState::Released,
                };
                if rig.hold_state(arrow.note) != Some(state) {
                    rig.set_hold_state(arrow.note, state);
                }
            }
        }
    }

    /// Refreshes and bounces a player's combo readout on their graded row.
    fn apply_combo(&mut self, player: PlayerId, combo: u32) {
        for display in &mut self.combos {
            if display.player != player {
                continue;
            }
            if combo > display.last_combo {
                display.bounce = COMBO_BOUNCE;
            }
            display.last_combo = combo;
            if combo == 0 {
                display.label.set_visible(false);
            } else {
                display.label.set_visible(true);
                display.label.set_text(&format!("{combo} combo"));
            }
        }
    }

    fn animate_hud(&mut self, delta: f64) {
        let settings = Settings::singleton();
        for grade in &mut self.grades {
            let position = settings.bind().player(grade.player).grade_position;
            let y = grade_y(&self.grade_area, position);
            grade.animate(delta as f32, y);
        }
        for combo in &mut self.combos {
            combo.bounce = (combo.bounce - Seconds(delta)).max(Seconds::ZERO);
            let scale = 1.0 + 0.22 * (combo.bounce / COMBO_BOUNCE) as f32;
            let position = settings.bind().player(combo.player).grade_position;
            let y = grade_y(&self.grade_area, position) - COMBO_GAP;
            place_label(
                &mut combo.label,
                Vector2::new(combo.origin_x, -y),
                TextPivot::CENTER,
            );
            let size = combo.label.get_size();
            combo.label.set_pivot_offset(size / 2.0);
            combo.label.set_scale(Vector2::splat(scale));
        }
        self.fades.retain_mut(|fade| {
            fade.remaining -= delta as f32;
            if fade.remaining <= 0.0 {
                fade.node.queue_free();
                return false;
            }
            let alpha = fade.remaining / fade.total;
            if fade.growth != 0.0 {
                fade.node
                    .set_scale(fade.base_scale * (1.0 + fade.growth * (1.0 - alpha)));
            }
            let mut modulate = fade.node.get_modulate();
            modulate.a = alpha;
            fade.node.set_modulate(modulate);
            true
        });
    }
}

#[godot_api]
impl IControl for StepfilePlayer {
    fn init(base: Base<Control>) -> StepfilePlayer {
        StepfilePlayer {
            timing: StepfileTiming::new(Seconds::ZERO, &[], &[]),
            canvas: crate::core::screen::SCREEN_SIZE,
            pixel_scale: 1.0,
            target_y: note_field::TARGET_Y,
            grade_area: GradeArea::default(),
            graded_now: Seconds::ZERO,
            visible_now: Seconds::ZERO,
            input: PlayInput::default(),
            stages: Vec::new(),
            rigs: Vec::new(),
            grades: Vec::new(),
            combos: Vec::new(),
            fades: Vec::new(),
            behind: None,
            overlay: None,
            last_note_time: Seconds::ZERO,
            base,
        }
    }

    fn process(&mut self, delta: f64) {
        // The adapter (our parent) filled the ports before this runs.
        let events = self.run_grading(delta);
        for event in events {
            match event {
                StageEvent::Graded {
                    player,
                    outcome,
                    combo,
                } => {
                    for grade in &mut self.grades {
                        if grade.player == player {
                            grade.apply(config(), outcome);
                        }
                    }
                    self.apply_combo(player, combo);
                }
                StageEvent::PressBanked { error } => {
                    self.signals().press_banked().emit(error.0);
                }
                StageEvent::Failed { player } => {
                    let index = match player {
                        PlayerId::P1 => 0,
                        PlayerId::P2 => 1,
                    };
                    self.signals().stage_failed().emit(index);
                }
            }
        }
        self.sync_fields();

        let center = self.canvas / 2.0;
        if let Some(behind) = &mut self.behind {
            behind.set_position(center);
        }
        if let Some(overlay) = &mut self.overlay {
            overlay.set_position(center);
        }
        let clock = FieldClock {
            visible: self.visible_now,
            timing: self.timing.clone(),
            target_y: self.target_y,
        };
        for rig in &mut self.rigs {
            rig.update(&clock, delta as f32);
        }
        self.animate_hud(delta);
    }

    fn exit_tree(&mut self) {
        for rig in &mut self.rigs {
            rig.free();
        }
    }
}
