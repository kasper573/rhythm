use super::note_skin::{ElementVisual, NOTE_CELL, NoteSkin, effect_material, tail_material};
use crate::core::config::GameConfig;
use crate::core::input::{GameAction, StepDirection};
use crate::core::player::PlayerId;
use crate::core::settings::{NoteSpeed, Perspective};
use crate::core::stepfile::StepfileTiming;
use crate::core::units::{Beat, Seconds};
use godot::classes::camera_3d::ProjectionType;
use godot::classes::control::LayoutPreset;
use godot::classes::{
    Camera3D, Control, MeshInstance3D, Node3D, StandardMaterial3D, SubViewport, TextureRect,
};
use godot::prelude::*;

/// The receptor row's y where no window overrides it (headless renderers),
/// in the lane world's y-up centered coordinates; live sessions re-anchor
/// every frame through their own window logic.
pub const TARGET_Y: f32 = 260.0;

/// Columns sit slightly further apart than the arrows are wide, keeping
/// the classic gap whatever size a field is scaled to.
const COLUMN_SPACING_RATIO: f32 = 100.0 / 88.0;

/// Columns on one physical pad; wider fields span several pads.
const PAD_COLUMNS: usize = 4;

/// The largest arrow size — capped at `max_size` (see [`max_arrow_size`])
/// — whose columns fit `spacing_units` column spacings into `available`
/// canvas width.
pub fn fitted_arrow_size(spacing_units: f32, available: f32, max_size: f32) -> f32 {
    (available / spacing_units / COLUMN_SPACING_RATIO).min(max_size)
}

/// The configured arrow-size cap — a *screen pixel* budget — as canvas
/// units: `pixels_per_unit` (the window's canvas scale) grows with the
/// window, so the canvas-unit cap shrinks to keep arrows at most the
/// configured pixel size on screen. Headless renderers keep the design
/// canvas's 1:1 scale.
pub fn max_arrow_size(config: &GameConfig, pixels_per_unit: f32) -> f32 {
    config.stage.max_arrow_size / pixels_per_unit.max(f32::MIN_POSITIVE)
}

/// One lane group on stage: a player's columns, centered on `origin_x`,
/// scrolling at that player's speed and drawn in their skin at the
/// field's arrow size. Coordinates are canvas-centered and y-up, matching
/// the lane's own 3D scene.
#[derive(Clone)]
pub struct FieldLayout {
    pub player: PlayerId,
    pub origin_x: f32,
    pub columns: usize,
    pub speed: NoteSpeed,
    pub arrow_size: f32,
}

impl FieldLayout {
    pub fn spacing(&self) -> f32 {
        self.arrow_size * COLUMN_SPACING_RATIO
    }

    pub fn width(&self) -> f32 {
        self.columns as f32 * self.spacing()
    }

    pub fn column_x(&self, column: usize) -> f32 {
        self.origin_x + (column as f32 - (self.columns as f32 - 1.0) / 2.0) * self.spacing()
    }

    /// The key that steps `column`: a field wider than one pad continues
    /// onto the second player's pad (doubles), otherwise every column
    /// belongs to the field's owner.
    pub fn step_action(&self, column: usize) -> GameAction {
        let side = if column < PAD_COLUMNS {
            self.player
        } else {
            PlayerId::P2
        };
        GameAction::step(side, StepDirection::of_column(column % PAD_COLUMNS))
    }
}

/// Paces every note-field animation: `visible` is the current moment on the
/// drawn timeline and `timing` converts it to beats — shared by every field
/// on stage, while speed and skin vary per field. The stage's owner
/// advances `visible` and anchors `target_y` (canvas-centered, y-up).
#[derive(Clone)]
pub struct FieldClock {
    pub visible: Seconds,
    pub timing: StepfileTiming,
    /// Lane-world y of the receptor row, where scrolling arrows arrive.
    pub target_y: f32,
}

impl FieldClock {
    pub fn beat(&self) -> Beat {
        self.timing.beat_at_seconds(self.visible)
    }

    fn scroll(&self) -> NoteScroll {
        NoteScroll {
            now: self.visible,
            now_beat: self.beat(),
            target_y: self.target_y,
        }
    }
}

/// A per-frame snapshot placing notes on screen: [`NoteSpeed::Constant`]
/// spaces notes by their seconds, [`NoteSpeed::Dynamic`] by their beats —
/// one arrow height per beat at multiplier 1, whatever the field's size.
struct NoteScroll {
    now: Seconds,
    now_beat: Beat,
    target_y: f32,
}

impl NoteScroll {
    fn y_at(&self, layout: &FieldLayout, time: Seconds, beat: Beat) -> f32 {
        let arrows_until = match layout.speed {
            NoteSpeed::Constant(scroll_bpm) => (time - self.now).0 * scroll_bpm as f64 / 60.0,
            NoteSpeed::Dynamic(multiplier) => (beat - self.now_beat).0 * multiplier as f64,
        };
        self.target_y - (arrows_until * layout.arrow_size as f64) as f32
    }

    /// Where arrows stop scrolling: pinned hold heads stick here.
    fn target_y(&self) -> f32 {
        self.target_y
    }
}

/// Render state of a hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HoldVisualState {
    /// Not yet stepped: scrolls by whole, inactive textures.
    #[default]
    Pending,
    /// Stepped and satisfied: head pinned at the receptor, active textures.
    Held,
    /// Stepped but the panel is up; still alive: pinned, inactive textures.
    Released,
    /// Kept to the end: body and cap disappear.
    Ok,
    /// Dropped, or the head was missed: dimmed, scrolls away.
    Dropped,
}

impl HoldVisualState {
    fn pinned(self) -> bool {
        matches!(self, HoldVisualState::Held | HoldVisualState::Released)
    }

    fn active(self) -> bool {
        matches!(self, HoldVisualState::Held)
    }
}

pub const HOLD_OK_FADE_SECONDS: f32 = 0.05;
const MINE_EXPLOSION_SECONDS: f32 = 0.4;
const PRESS_SECONDS: f32 = 0.25;

/// The body slides this far under the cap, so the cap's filtered top edge
/// blends into the body instead of the background.
const BODY_CAP_OVERLAP: f32 = 1.0;

/// Notes spawn far off-screen and are placed by the scroll from their
/// first frame.
const OFFSCREEN_Y: f32 = -10_000.0;

pub struct NoteSpawn {
    pub time: Seconds,
    pub beat: Beat,
    pub column: usize,
    /// Recognized quantization (see `GameConfig::recognized_quant`).
    pub quant: u32,
    pub tail: Option<NoteTail>,
}

/// A hold or roll note's tail.
pub struct NoteTail {
    pub time: Seconds,
    pub beat: Beat,
    pub roll: bool,
}

/// A spawned note's handle within its field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteIndex(pub usize);

/// A spawned mine's handle within its field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MineIndex(pub usize);

struct ReceptorEl {
    node: Gd<MeshInstance3D>,
    column: usize,
    cell: f32,
    held: bool,
    press: f32,
}

struct NoteEl {
    node: Gd<MeshInstance3D>,
    time: Seconds,
    beat: Beat,
    column: usize,
    cell: f32,
    /// The skin row of this note's quant, for hold-head material swaps.
    skin_row: usize,
    hold: Option<HoldEl>,
    /// Vanished or fading notes leave the scroll.
    live: bool,
}

struct HoldEl {
    end: Seconds,
    end_beat: Beat,
    roll: bool,
    state: HoldVisualState,
    body: Gd<MeshInstance3D>,
    body_material: Gd<StandardMaterial3D>,
    cap: Gd<MeshInstance3D>,
    cap_material: Gd<StandardMaterial3D>,
}

struct MineEl {
    node: Gd<MeshInstance3D>,
    time: Seconds,
    beat: Beat,
    live: bool,
}

/// One fading lane element: shrinks/grows while its owned material's alpha
/// (when it has one of its own) runs out, then frees itself.
struct FadingElement {
    node: Gd<MeshInstance3D>,
    material: Option<Gd<StandardMaterial3D>>,
    remaining: f32,
    total: f32,
    growth: f32,
    base_scale: Vector3,
    base_color: Color,
}

/// One player's note field: its own little 3D scene — receptors, notes,
/// holds, mines, and transient effects — rendered by a perspective camera
/// into a transparent viewport composited into the stage. The camera
/// hovers over the field's center where a flat view reproduces the canvas
/// 1:1 on the lane plane, with its frustum window shifted back over the
/// canvas rect, so every field keeps its own vanishing point; the player's
/// perspective pitches it around the receptor row.
pub struct NoteFieldRig {
    pub layout: FieldLayout,
    pub skin: NoteSkin,
    perspective: Perspective,
    viewport: Gd<SubViewport>,
    display: Gd<TextureRect>,
    camera: Gd<Camera3D>,
    space: Gd<Node3D>,
    fov: f32,
    tilt: f32,
    canvas: Vector2,
    elapsed: f64,
    receptors: Vec<ReceptorEl>,
    notes: Vec<NoteEl>,
    mines: Vec<MineEl>,
    fades: Vec<FadingElement>,
}

impl NoteFieldRig {
    /// Builds an empty field into `parent`: the viewport, its camera, and
    /// the receptor row. `canvas` is the design rect the lane maps 1:1.
    pub fn build(
        parent: &mut Control,
        layout: FieldLayout,
        skin: NoteSkin,
        perspective: Perspective,
        fov_degrees: f32,
        tilt_degrees: f32,
        canvas: Vector2,
    ) -> NoteFieldRig {
        let mut viewport = SubViewport::new_alloc();
        viewport.set_transparent_background(true);
        viewport.set_size(Vector2i::new(canvas.x as i32, canvas.y as i32));
        viewport.set_msaa_3d(godot::classes::viewport::Msaa::MSAA_4X);
        viewport.set_update_mode(godot::classes::sub_viewport::UpdateMode::ALWAYS);
        let mut space = Node3D::new_alloc();
        viewport.add_child(&space);
        let mut camera = Camera3D::new_alloc();
        camera.set_projection(ProjectionType::FRUSTUM);
        space.add_child(&camera);
        parent.add_child(&viewport);

        let mut display = TextureRect::new_alloc();
        display.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        display.set_stretch_mode(godot::classes::texture_rect::StretchMode::SCALE);
        display.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
        display.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
        if let Some(texture) = viewport.get_texture() {
            display.set_texture(&texture);
        }
        parent.add_child(&display);

        let mut rig = NoteFieldRig {
            layout,
            skin,
            perspective,
            viewport,
            display,
            camera,
            space,
            fov: fov_degrees.to_radians(),
            tilt: tilt_degrees.to_radians(),
            canvas,
            elapsed: 0.0,
            receptors: Vec::new(),
            notes: Vec::new(),
            mines: Vec::new(),
            fades: Vec::new(),
        };
        rig.spawn_receptors();
        rig.sync_camera(TARGET_Y);
        rig
    }

    /// Tears the whole field out of the tree.
    pub fn free(&mut self) {
        self.viewport.queue_free();
        self.display.queue_free();
    }

    /// Re-sizes the lane's canvas and its render resolution: `pixel_scale`
    /// is the window's canvas-to-pixel factor, so the lane renders at
    /// native resolution however the window is scaled.
    pub fn set_canvas(&mut self, canvas: Vector2, pixel_scale: f32) {
        let size = Vector2i::new(
            (canvas.x * pixel_scale).round().max(1.0) as i32,
            (canvas.y * pixel_scale).round().max(1.0) as i32,
        );
        if self.canvas != canvas || self.viewport.get_size() != size {
            self.canvas = canvas;
            self.viewport.set_size(size);
        }
    }

    fn spawn_receptors(&mut self) {
        for column in 0..self.layout.columns {
            let visual = self.skin.receptor_visual();
            let cell = visual.cell;
            let node = self.spawn_element(visual, column, TARGET_Y, 10.0, column_rotation(column));
            self.receptors.push(ReceptorEl {
                node,
                column,
                cell,
                held: false,
                press: 0.0,
            });
        }
    }

    pub fn spawn_note(&mut self, note: &NoteSpawn) -> NoteIndex {
        let skin_row = self.skin.note.quant_row(note.quant);
        let rotation = column_rotation(note.column);
        let nudge = beat_z_nudge(note.beat);

        let (visual, hold) = match &note.tail {
            None => (self.skin.tap_visual(skin_row), None),
            Some(tail) => {
                let art = if tail.roll {
                    self.skin.roll.clone()
                } else {
                    self.skin.hold.clone()
                };
                let body_material = tail_material(&art.body_inactive);
                let cap_material = tail_material(&art.cap_inactive);
                let mut body = MeshInstance3D::new_alloc();
                body.set_mesh(&self.skin.quad_visual(body_material.clone()).mesh);
                body.set_surface_override_material(0, &body_material);
                body.set_position(Vector3::new(0.0, OFFSCREEN_Y, 18.0 - nudge));
                body.set_visible(false);
                let mut cap = MeshInstance3D::new_alloc();
                cap.set_mesh(&self.skin.quad_visual(cap_material.clone()).mesh);
                cap.set_surface_override_material(0, &cap_material);
                cap.set_position(Vector3::new(0.0, OFFSCREEN_Y, 18.2 - nudge));
                cap.set_visible(false);
                self.space.add_child(&body);
                self.space.add_child(&cap);
                (
                    self.skin.head_visual(skin_row, false),
                    Some(HoldEl {
                        end: tail.time,
                        end_beat: tail.beat,
                        roll: tail.roll,
                        state: HoldVisualState::default(),
                        body,
                        body_material,
                        cap,
                        cap_material,
                    }),
                )
            }
        };
        let cell = visual.cell;
        let node = self.spawn_element(visual, note.column, OFFSCREEN_Y, 20.0 - nudge, rotation);
        self.notes.push(NoteEl {
            node,
            time: note.time,
            beat: note.beat,
            column: note.column,
            cell,
            skin_row,
            hold,
            live: true,
        });
        NoteIndex(self.notes.len() - 1)
    }

    pub fn spawn_mine(&mut self, time: Seconds, beat: Beat, column: usize) -> MineIndex {
        let visual = self.skin.mine_visual();
        let node = self.spawn_element(
            visual,
            column,
            OFFSCREEN_Y,
            20.0 - beat_z_nudge(beat),
            Quaternion::default(),
        );
        self.mines.push(MineEl {
            node,
            time,
            beat,
            live: true,
        });
        MineIndex(self.mines.len() - 1)
    }

    fn spawn_element(
        &mut self,
        visual: ElementVisual,
        column: usize,
        y: f32,
        z: f32,
        rotation: Quaternion,
    ) -> Gd<MeshInstance3D> {
        let mut node = MeshInstance3D::new_alloc();
        node.set_mesh(&visual.mesh);
        node.set_surface_override_material(0, &visual.material);
        if let Some(shell) = &visual.shell {
            node.set_surface_override_material(1, shell);
        }
        let scale = self.layout.arrow_size / visual.cell;
        node.set_transform(Transform3D {
            basis: Basis::from_quaternion(rotation).scaled(Vector3::splat(scale)),
            origin: Vector3::new(self.layout.column_x(column), y, z),
        });
        self.space.add_child(&node);
        node
    }

    /// Whether the panel of `column` renders pressed.
    pub fn set_receptor_held(&mut self, column: usize, held: bool) {
        for receptor in &mut self.receptors {
            if receptor.column == column {
                receptor.held = held;
            }
        }
    }

    pub fn hold_state(&self, note: NoteIndex) -> Option<HoldVisualState> {
        self.notes[note.0].hold.as_ref().map(|hold| hold.state)
    }

    pub fn set_hold_state(&mut self, note: NoteIndex, state: HoldVisualState) {
        if let Some(hold) = &mut self.notes[note.0].hold {
            hold.state = state;
        }
    }

    /// Fades the note's head out where it stands (the hold-OK fade).
    pub fn fade_out_note(&mut self, note: NoteIndex, seconds: f32) {
        let el = &mut self.notes[note.0];
        if !el.live {
            return;
        }
        el.live = false;
        self.fades.push(FadingElement {
            node: el.node.clone(),
            material: None,
            remaining: seconds,
            total: seconds,
            growth: 0.0,
            base_scale: el.node.get_scale(),
            base_color: Color::WHITE,
        });
        if let Some(hold) = &mut el.hold {
            hold.body.queue_free();
            hold.cap.queue_free();
            el.hold = None;
        }
    }

    /// Despawns the note on the spot, as grading does for vanished taps.
    pub fn vanish_note(&mut self, note: NoteIndex) {
        let el = &mut self.notes[note.0];
        if !el.live {
            return;
        }
        el.live = false;
        el.node.queue_free();
        if let Some(hold) = &mut el.hold {
            hold.body.queue_free();
            hold.cap.queue_free();
            el.hold = None;
        }
    }

    pub fn remove_mine(&mut self, mine: MineIndex) {
        let el = &mut self.mines[mine.0];
        if el.live {
            el.live = false;
            el.node.queue_free();
        }
    }

    /// The arrow flash at a receptor when a step's arrows vanish, growing
    /// while it fades. The bright variant plays at high combo: larger
    /// art, snappier, starting smaller.
    pub fn arrow_flash(&mut self, column: usize, target_y: f32, color: Color, bright: bool) {
        let (flash, seconds, base_zoom, growth) = if bright {
            (self.skin.flash_bright.clone(), 0.13, 0.8, 0.5)
        } else {
            (self.skin.flash_dim.clone(), 0.18, 1.0, 0.4)
        };
        let size = flash.size * (self.layout.arrow_size / NOTE_CELL) * base_zoom;
        self.effect(
            effect_material(&flash.texture, color),
            Transform3D {
                basis: Basis::from_quaternion(column_rotation(column))
                    .scaled(Vector3::new(size.x, size.y, 1.0)),
                origin: Vector3::new(self.layout.column_x(column), target_y, 22.0),
            },
            seconds,
            growth,
        );
    }

    pub fn mine_explosion(&mut self, column: usize, target_y: f32) {
        let scale = self.layout.arrow_size * 1.7;
        let texture = self.skin.mine_explosion.texture.clone();
        self.effect(
            effect_material(&texture, Color::WHITE),
            Transform3D {
                basis: Basis::from_scale(Vector3::splat(scale)),
                origin: Vector3::new(self.layout.column_x(column), target_y, 21.0),
            },
            MINE_EXPLOSION_SECONDS,
            0.25,
        );
    }

    fn effect(
        &mut self,
        material: Gd<StandardMaterial3D>,
        transform: Transform3D,
        seconds: f32,
        growth: f32,
    ) {
        let mut node = MeshInstance3D::new_alloc();
        node.set_mesh(&self.skin.quad_visual(material.clone()).mesh);
        node.set_surface_override_material(0, &material);
        node.set_transform(transform);
        self.space.add_child(&node);
        self.fades.push(FadingElement {
            node,
            material: Some(material.clone()),
            remaining: seconds,
            total: seconds,
            growth,
            base_scale: transform.basis.get_scale(),
            base_color: material.get_albedo(),
        });
    }

    /// The whole field shuts down: notes, mines, and receptors shrink and
    /// fade away.
    pub fn fail_out(&mut self, seconds: f32) {
        let mut doomed: Vec<Gd<MeshInstance3D>> = Vec::new();
        for note in &mut self.notes {
            if note.live {
                note.live = false;
                doomed.push(note.node.clone());
                if let Some(hold) = &mut note.hold {
                    doomed.push(hold.body.clone());
                    doomed.push(hold.cap.clone());
                    note.hold = None;
                }
            }
        }
        for mine in &mut self.mines {
            if mine.live {
                mine.live = false;
                doomed.push(mine.node.clone());
            }
        }
        for receptor in std::mem::take(&mut self.receptors) {
            doomed.push(receptor.node);
        }
        for node in doomed {
            self.fades.push(FadingElement {
                base_scale: node.get_scale(),
                node,
                material: None,
                remaining: seconds,
                total: seconds,
                growth: -1.0,
                base_color: Color::WHITE,
            });
        }
    }

    /// One animation frame: scroll, textures, receptor press, hold parts,
    /// mines, transient fades, and the camera.
    pub fn update(&mut self, clock: &FieldClock, delta: f32) {
        let scroll = clock.scroll();
        let beat = clock.beat();

        self.elapsed += delta as f64;
        self.animate_sheet_taps(beat);
        self.animate_receptor_frames(beat);
        self.animate_receptor_press(delta);
        self.scroll_and_animate_notes(&scroll);
        self.animate_mines(&scroll, beat);
        self.scroll_model_textures();
        self.run_fades(delta);
        self.sync_camera(clock.target_y);
    }

    fn scroll_and_animate_notes(&mut self, scroll: &NoteScroll) {
        for el in &mut self.notes {
            if !el.live {
                continue;
            }
            let pinned = el.hold.as_ref().is_some_and(|hold| hold.state.pinned());
            let mut y = scroll.y_at(&self.layout, el.time, el.beat);
            if pinned {
                y = y.min(scroll.target_y());
            }
            let x = self.layout.column_x(el.column);
            let scale = self.layout.arrow_size / el.cell;
            let mut position = el.node.get_position();
            position.x = x;
            position.y = y;
            el.node.set_position(position);
            let wanted = Vector3::splat(scale);
            if el.node.get_scale() != wanted {
                el.node.set_scale(wanted);
            }

            let Some(hold) = &mut el.hold else { continue };
            // Hold heads swap material with the hold's state.
            let visual = self.skin.head_visual(el.skin_row, hold.state.active());
            el.node.set_surface_override_material(0, &visual.material);

            animate_hold_parts(&self.layout, &self.skin, el.column, y, scroll, hold);
        }
    }

    fn animate_sheet_taps(&mut self, beat: Beat) {
        let super::note_skin::NoteArt::Sheet(sheet) = &self.skin.note else {
            return;
        };
        let x = sheet.frame_x_at(beat);
        for material in sheet.tap_materials() {
            let mut material = material.clone();
            let mut offset = material.get_uv1_offset();
            if offset.x != x {
                offset.x = x;
                material.set_uv1_offset(offset);
            }
        }
    }

    fn animate_receptor_frames(&mut self, beat: Beat) {
        let receptor = &self.skin.receptor;
        let x = receptor.frame_x_at(beat);
        let brightness = receptor.brightness_at(beat);
        let color = Color::from_rgb(brightness, brightness, brightness);
        let mut material = receptor.material.clone();
        let mut offset = material.get_uv1_offset();
        if offset.x != x || material.get_albedo() != color {
            offset.x = x;
            material.set_uv1_offset(offset);
            material.set_albedo(color);
        }
    }

    /// Held receptors tween back along Z with a shrink to sell the depth.
    fn animate_receptor_press(&mut self, delta: f32) {
        for receptor in &mut self.receptors {
            if !receptor.held && receptor.press == 0.0 {
                continue;
            }
            let step = delta / PRESS_SECONDS;
            let step = if receptor.held { step } else { -step };
            receptor.press = (receptor.press + step).clamp(0.0, 1.0);
            let eased = ease_cubic_in_out(receptor.press);
            let base = self.layout.arrow_size / receptor.cell;
            let mut position = receptor.node.get_position();
            position.z = 10.0 - 6.0 * eased;
            receptor.node.set_position(position);
            receptor
                .node
                .set_scale(Vector3::splat(base * (1.0 - 0.22 * eased)));
        }
    }

    fn animate_mines(&mut self, scroll: &NoteScroll, beat: Beat) {
        let spin = self.skin.mine_spin_beats;
        for mine in &mut self.mines {
            if !mine.live {
                continue;
            }
            let y = scroll.y_at(&self.layout, mine.time, mine.beat);
            let angle = (-(beat.0.rem_euclid(spin) / spin) * std::f64::consts::TAU) as f32;
            let scale = self.layout.arrow_size / NOTE_CELL;
            let mut position = mine.node.get_position();
            position.y = y;
            mine.node.set_position(position);
            mine.node.set_basis(
                Basis::from_axis_angle(Vector3::BACK, angle).scaled(Vector3::splat(scale)),
            );
        }
    }

    /// Drifts the texture coordinates of every scrolling model material —
    /// the classic animated color strips of 3D note skins. Derived from
    /// unwrapped f64 time, so it never pops at a wrap seam.
    fn scroll_model_textures(&mut self) {
        let elapsed = self.elapsed;
        for (mut material, base, velocity) in self.skin.scrolling_materials() {
            if velocity == Vector2::ZERO {
                continue;
            }
            let scroll = |base: f32, velocity: f32| {
                (base as f64 + velocity as f64 * elapsed).rem_euclid(1.0) as f32
            };
            material.set_uv1_offset(Vector3::new(
                scroll(base.x, velocity.x),
                scroll(base.y, velocity.y),
                0.0,
            ));
        }
    }

    fn run_fades(&mut self, delta: f32) {
        self.fades.retain_mut(|fade| {
            fade.remaining -= delta;
            if fade.remaining <= 0.0 {
                fade.node.queue_free();
                return false;
            }
            let alpha = fade.remaining / fade.total;
            if fade.growth != 0.0 {
                fade.node
                    .set_scale(fade.base_scale * (1.0 + fade.growth * (1.0 - alpha)));
            }
            if let Some(material) = &mut fade.material {
                let mut color = fade.base_color;
                color.a *= alpha;
                material.set_albedo(color);
            }
            true
        });
    }

    /// Keeps the lane camera over the field's center, pitched around the
    /// receptor row per the player's perspective, with the frustum window
    /// shifted back over the canvas rect.
    fn sync_camera(&mut self, target_y: f32) {
        let distance = self.canvas.y * 0.5 / (self.fov * 0.5).tan();
        let near = distance * 0.05;
        let far = distance * 4.0;
        let tilt = match self.perspective {
            Perspective::None => 0.0,
            Perspective::Above => -self.tilt,
            Perspective::Below => self.tilt,
        };
        let origin_x = self.layout.origin_x;
        let pivot = Vector3::new(origin_x, target_y, 0.0);
        let rotation = Basis::from_axis_angle(Vector3::RIGHT, tilt);
        let position = pivot + rotation * (Vector3::new(origin_x, 0.0, distance) - pivot);
        self.camera.set_transform(Transform3D {
            basis: rotation,
            origin: position,
        });
        self.camera.set_near(near);
        self.camera.set_far(far);
        // Frustum size is the window height at the near plane.
        self.camera.set_size(2.0 * near * (self.fov * 0.5).tan());
        // Shift the projection window back over the canvas rect the stage
        // composites, so off-center fields keep their own vanishing point.
        self.camera
            .set_frustum_offset(Vector2::new(-origin_x * near / distance, 0.0));
    }
}

/// Overlapping lane elements at one depth would tie in the transparent
/// sort and flicker as their draw order swaps between frames — under a
/// flat camera, every note shares a single view depth. Each note sits
/// slightly deeper the later its beat, so an earlier note always draws
/// over the ones scrolling in behind it. Only co-visible notes need
/// distinct depths, so the nudge wraps: the period is far beyond any
/// on-screen beat span, and the bounded range keeps even marathon charts'
/// notes inside their own layer band.
fn beat_z_nudge(beat: Beat) -> f32 {
    (beat.0.rem_euclid(256.0) * 0.005) as f32
}

/// The skin's arrows point down; rotate per pad-local column so every
/// group of four reads Left, Down, Up, Right (doubles repeats the cycle
/// on the second pad).
pub fn column_rotation(column: usize) -> Quaternion {
    let angle = match column % PAD_COLUMNS {
        0 => -std::f32::consts::FRAC_PI_2,
        1 => 0.0,
        2 => std::f32::consts::PI,
        _ => std::f32::consts::FRAC_PI_2,
    };
    Quaternion::from_axis_angle(Vector3::BACK, angle)
}

fn ease_cubic_in_out(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// Positions and styles the hold tail: the body is one quad whose texture
/// wraps vertically, anchored to the tail so the pattern always meets the
/// cap at a tile boundary, and the cap sits centered on the tail below it —
/// clipped so nothing draws above the head's center. Each part owns its
/// material: the texture switches between active and inactive with the
/// hold's state, the texture window drives the tiling and clipping, and
/// dropped holds dim to the skin's NG brightness.
fn animate_hold_parts(
    layout: &FieldLayout,
    skin: &NoteSkin,
    column: usize,
    head_y: f32,
    scroll: &NoteScroll,
    hold: &mut HoldEl,
) {
    let art = if hold.roll { &skin.roll } else { &skin.hold };
    let scale = layout.arrow_size / art.body_size.x;
    let cap_height = art.cap_size.y * scale;
    if hold.state == HoldVisualState::Ok {
        hold.body.set_visible(false);
        hold.cap.set_visible(false);
        return;
    }

    let end_y = scroll.y_at(layout, hold.end, hold.end_beat);
    let body_bottom = end_y + art.stop_above_tail * scale;
    let x = layout.column_x(column);

    let active = hold.state.active();
    let brightness = if hold.state == HoldVisualState::Dropped {
        skin.dropped_brightness
    } else {
        1.0
    };
    let color = Color::from_rgb(brightness, brightness, brightness);

    let length = head_y - body_bottom;
    if length <= 0.5 {
        hold.body.set_visible(false);
    } else {
        let height = length + BODY_CAP_OVERLAP;
        let body_z = hold.body.get_position().z;
        // The texture window ends on the texture's bottom edge so the
        // pattern is anchored to the tail, whatever the length; the wrap
        // sampler tiles it upward from there.
        let top = art.body_size.y - length / scale;
        let bottom = art.body_size.y + BODY_CAP_OVERLAP / scale;
        apply_part(
            &mut hold.body,
            &mut hold.body_material,
            if active {
                &art.body_active
            } else {
                &art.body_inactive
            },
            uv_window(top, bottom, art.body_size.y),
            color,
            Vector3::new(x, head_y - height / 2.0, body_z),
            Vector3::new(layout.arrow_size, height, 1.0),
        );
    }

    // Clipped at the head's center; the bottom of the texture stays, so
    // the tail keeps its tip.
    let top = body_bottom.min(head_y);
    let bottom = body_bottom - cap_height;
    let visible = (top - bottom).min(cap_height);
    if visible <= 0.5 {
        hold.cap.set_visible(false);
    } else {
        let hidden = (art.cap_size.y - visible / scale).max(0.0);
        let cap_z = hold.cap.get_position().z;
        apply_part(
            &mut hold.cap,
            &mut hold.cap_material,
            if active {
                &art.cap_active
            } else {
                &art.cap_inactive
            },
            uv_window(hidden, art.cap_size.y, art.cap_size.y),
            color,
            Vector3::new(x, bottom + visible / 2.0, cap_z),
            Vector3::new(layout.arrow_size, visible, 1.0),
        );
    }
}

/// The vertical texture window `top..bottom` (in texture pixels of a
/// `texture_height` tall image) as `(uv scale, uv offset)` for a unit quad.
fn uv_window(top: f32, bottom: f32, texture_height: f32) -> (Vector2, Vector2) {
    (
        Vector2::new(1.0, (bottom - top) / texture_height),
        Vector2::new(0.0, top / texture_height),
    )
}

fn apply_part(
    node: &mut Gd<MeshInstance3D>,
    material: &mut Gd<StandardMaterial3D>,
    texture: &Gd<godot::classes::Texture2D>,
    (uv_scale, uv_offset): (Vector2, Vector2),
    color: Color,
    position: Vector3,
    scale: Vector3,
) {
    material.set_texture(
        godot::classes::base_material_3d::TextureParam::ALBEDO,
        texture,
    );
    material.set_uv1_scale(Vector3::new(uv_scale.x, uv_scale.y, 1.0));
    material.set_uv1_offset(Vector3::new(uv_offset.x, uv_offset.y, 0.0));
    material.set_albedo(color);
    node.set_position(position);
    node.set_scale(scale);
    node.set_visible(true);
}
