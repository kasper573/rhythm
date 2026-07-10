use super::note_skin::{
    ActiveNoteSkin, ActiveNoteSkins, ElementVisual, NOTE_CELL, NoteArt, effect_material,
    tail_material,
};
use crate::core::config::GameConfig;
use crate::core::input::{GameAction, StepDirection};
use crate::core::player::PlayerId;
use crate::core::settings::{NoteSpeed, Perspective, PlayerSettings};
use crate::core::stepfile::StepfileTiming;
use crate::core::units::{Beat, Seconds};
use crate::core::{LANE_LAYER_BASE, SCREEN_SIZE, visible_world_size};
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{CameraProjection, ClearColorConfig, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::ecs::query::QueryData;
use bevy::math::{Affine2, Vec3A};
use bevy::prelude::*;
use strum::IntoEnumIterator;

/// The receptor row's y where no window overrides it (headless renderers);
/// live sessions re-anchor every frame through their own window logic.
pub const TARGET_Y: f32 = 260.0;

/// Columns sit slightly further apart than the arrows are wide, keeping
/// the classic gap whatever size a field is scaled to.
const COLUMN_SPACING_RATIO: f32 = 100.0 / 88.0;

/// Columns on one physical pad; wider fields span several pads.
const PAD_COLUMNS: usize = 4;

/// The largest arrow size — capped at `max_size` (see [`max_arrow_size`])
/// — whose columns fit `spacing_units` column spacings into `available`
/// world width.
pub fn fitted_arrow_size(spacing_units: f32, available: f32, max_size: f32) -> f32 {
    (available / spacing_units / COLUMN_SPACING_RATIO).min(max_size)
}

/// The configured arrow-size cap — a *screen pixel* budget — as world
/// units on `window`: the world-to-pixel scale grows with the window, so
/// the world-unit cap shrinks to keep arrows at most the configured pixel
/// size on screen. Headless renderers (no window) keep the design
/// canvas's 1:1 scale.
pub fn max_arrow_size(config: &GameConfig, window: Option<&Window>) -> f32 {
    let pixels_per_world = window
        .map(|window| window.width().max(1.0) / visible_world_size(window).x)
        .unwrap_or(1.0);
    config.stage.max_arrow_size / pixels_per_world
}

/// One lane group on stage: a player's columns, centered on `origin_x`,
/// scrolling at that player's speed and drawn in their skin at the
/// field's arrow size. Receptors, notes, and mines belong to a field
/// through [`InField`]; a play session spawns one field per player
/// (doubles is one player's single 8-column field).
///
/// Every field is a little 3D scene: its entities live on the lane's own
/// render layer, drawn by a matching lane camera whose pitch applies the
/// player's [`Perspective`] (see [`sync_lane_cameras`]).
#[derive(Component, Clone)]
pub struct NoteField {
    pub player: PlayerId,
    /// The field's index on stage: picks its render layer and camera order.
    pub lane: usize,
    pub origin_x: f32,
    pub columns: usize,
    pub speed: NoteSpeed,
    pub arrow_size: f32,
    /// Where this field's lane camera renders and the canvas it maps there:
    /// the window for the play stage, an offscreen image for headless
    /// renderers and the embedded options preview.
    pub view: LaneView,
}

impl NoteField {
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

    fn render_layers(&self) -> RenderLayers {
        RenderLayers::layer(LANE_LAYER_BASE + self.lane)
    }
}

/// The field an on-stage entity belongs to.
#[derive(Component, Clone, Copy)]
pub struct InField(pub Entity);

/// Paces every note-field animation: `visible` is the current moment on the
/// drawn timeline and `timing` converts it to beats — shared by every field
/// on stage, while speed and skin vary per field. The field systems run
/// only while this resource exists. The owner of the stage inserts it,
/// advances `visible`, anchors `target_y`, and flips the state components
/// ([`Receptor::held`], [`HoldVisual`], [`FadeOut`]) — gameplay rules stay
/// with the owner.
#[derive(Resource)]
pub struct NoteFieldClock {
    pub visible: Seconds,
    pub timing: StepfileTiming,
    /// World y of the receptor row, where scrolling arrows arrive.
    pub target_y: f32,
}

impl NoteFieldClock {
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

/// What the lane cameras must line up with: where they draw, and the
/// design canvas the world camera keeps visible there (`AutoMin`), so the
/// lane plane renders 1:1 with the 2D world. The game's default is the
/// primary window and its 1280x720 canvas; headless renderers and embedded
/// previews point it at their own image, whose world is the image itself.
#[derive(Clone)]
pub struct LaneView {
    pub target: RenderTarget,
    pub canvas: Vec2,
}

impl Default for LaneView {
    fn default() -> LaneView {
        LaneView {
            target: RenderTarget::default(),
            canvas: SCREEN_SIZE,
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
    fn y_at(&self, field: &NoteField, time: Seconds, beat: Beat) -> f32 {
        let arrows_until = match field.speed {
            NoteSpeed::Constant(scroll_bpm) => (time - self.now).0 * scroll_bpm as f64 / 60.0,
            NoteSpeed::Dynamic(multiplier) => (beat - self.now_beat).0 * multiplier as f64,
        };
        self.target_y - (arrows_until * field.arrow_size as f64) as f32
    }

    /// Where arrows stop scrolling: pinned hold heads stick here.
    fn target_y(&self) -> f32 {
        self.target_y
    }
}

/// Lane-space placement of a field element: the column it sits on and its
/// mesh's authored cell size. [`place_field_elements`] keeps world x and
/// footprint derived from these and the field, so a refit (the window
/// rescaling the pixel budget) moves whole lanes without respawning them.
#[derive(Component)]
pub struct InColumn {
    pub column: usize,
    cell: f32,
}

#[derive(Component, Default)]
pub struct Receptor {
    /// The press tween follows this.
    pub held: bool,
    press: f32,
}

#[derive(Component, Clone)]
pub struct NoteArrow {
    pub time: Seconds,
    pub beat: Beat,
}

/// An arrow drawn as the skin's hold head, switching with the hold's state.
#[derive(Component, Clone)]
pub struct HoldHead {
    pub row: usize,
}

/// Render state of a hold, on the same entity as its head arrow.
#[derive(Component, Clone)]
pub struct HoldVisual {
    pub end: Seconds,
    pub end_beat: Beat,
    pub state: HoldVisualState,
}

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

#[derive(Component, Clone)]
pub struct HoldPart {
    pub head: Entity,
    pub piece: HoldPiece,
    pub roll: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldPiece {
    Body,
    Cap,
}

#[derive(Component, Clone)]
pub struct MineNote {
    pub time: Seconds,
    pub beat: Beat,
}

pub const HOLD_OK_FADE_SECONDS: f32 = 0.05;
const MINE_EXPLOSION_SECONDS: f32 = 0.4;

/// Marks a fading entity whose material is exclusively its own, letting
/// [`fade_out`] drive the material's alpha. Entities on shared skin
/// materials must not carry this — they rely on the scale change instead.
#[derive(Component)]
pub struct FadesMaterial;

/// Fades the entity out where it stands, then despawns it; fading arrows
/// stop scrolling because [`scroll_arrows`] skips them. Sprites, text,
/// and [`FadesMaterial`] entities fade their alpha; everything else
/// relies on the scale change (`growth` of -1 shrinks to nothing).
#[derive(Component)]
pub struct FadeOut {
    remaining: f32,
    total: f32,
    growth: f32,
    base_scale: Option<Vec3>,
}

impl FadeOut {
    pub fn over(seconds: f32) -> FadeOut {
        FadeOut {
            remaining: seconds,
            total: seconds,
            growth: 0.0,
            base_scale: None,
        }
    }

    /// Grows to `1 + growth` times its spawn size while fading.
    pub fn growing(seconds: f32, growth: f32) -> FadeOut {
        FadeOut {
            growth,
            ..FadeOut::over(seconds)
        }
    }
}

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

pub struct SpawnedNote {
    pub head: Entity,
    /// The hold's tail pieces; empty for taps.
    pub parts: Vec<Entity>,
}

impl SpawnedNote {
    pub fn entities(&self) -> impl Iterator<Item = Entity> {
        std::iter::once(self.head).chain(self.parts.iter().copied())
    }
}

pub fn spawn_note_field(commands: &mut Commands, field: NoteField) -> Entity {
    commands.spawn(field).id()
}

pub fn spawn_receptors(
    commands: &mut Commands,
    skin: &ActiveNoteSkin,
    field: Entity,
    layout: &NoteField,
) -> Vec<Entity> {
    (0..layout.columns)
        .map(|column| {
            let entity = spawn_element(
                commands,
                skin.receptor_visual(),
                layout,
                column,
                TARGET_Y,
                10.0,
                column_rotation(column),
            );
            commands
                .entity(entity)
                .insert((Receptor::default(), InField(field)));
            entity
        })
        .collect()
}

pub fn spawn_note(
    commands: &mut Commands,
    asset_server: &AssetServer,
    skin: &ActiveNoteSkin,
    field: Entity,
    layout: &NoteField,
    note: &NoteSpawn,
) -> SpawnedNote {
    let row = skin.note.quant_row(note.quant);
    let arrow = NoteArrow {
        time: note.time,
        beat: note.beat,
    };
    let rotation = column_rotation(note.column);

    let nudge = beat_z_nudge(note.beat);
    let head = match &note.tail {
        None => {
            let visual = skin.tap_visual(row);
            let head = spawn_element(
                commands,
                visual,
                layout,
                note.column,
                OFFSCREEN_Y,
                20.0 - nudge,
                rotation,
            );
            commands.entity(head).insert((arrow, InField(field)));
            head
        }
        Some(tail) => {
            let visual = skin.head_visual(row, false);
            let head = spawn_element(
                commands,
                visual,
                layout,
                note.column,
                OFFSCREEN_Y,
                20.0 - nudge,
                rotation,
            );
            commands.entity(head).insert((
                arrow,
                InField(field),
                HoldHead { row },
                HoldVisual {
                    end: tail.time,
                    end_beat: tail.beat,
                    state: HoldVisualState::default(),
                },
            ));
            head
        }
    };

    let mut parts = Vec::new();
    if let Some(tail) = &note.tail {
        let art = if tail.roll { &skin.roll } else { &skin.hold };
        for (piece, texture, z) in [
            (HoldPiece::Body, &art.body_inactive, 18.0),
            (HoldPiece::Cap, &art.cap_inactive, 18.2),
        ] {
            let material = asset_server.add(tail_material(texture.clone()));
            let entity = spawn_element(
                commands,
                skin.quad_visual(material),
                layout,
                note.column,
                OFFSCREEN_Y,
                z - nudge,
                Quat::IDENTITY,
            );
            commands.entity(entity).insert((
                HoldPart {
                    head,
                    piece,
                    roll: tail.roll,
                },
                InField(field),
                Visibility::Hidden,
            ));
            parts.push(entity);
        }
    }

    SpawnedNote { head, parts }
}

pub fn spawn_mine(
    commands: &mut Commands,
    skin: &ActiveNoteSkin,
    field: Entity,
    layout: &NoteField,
    time: Seconds,
    beat: Beat,
    column: usize,
) -> Entity {
    let entity = spawn_element(
        commands,
        skin.mine_visual(),
        layout,
        column,
        OFFSCREEN_Y,
        20.0 - beat_z_nudge(beat),
        Quat::IDENTITY,
    );
    commands
        .entity(entity)
        .insert((MineNote { time, beat }, InField(field)));
    entity
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

/// Spawns transient effects into a field's lane scene: tinted quads whose
/// own materials fade with their [`FadeOut`], drawn above everything in
/// the lane.
pub struct LaneEffects<'a, 'w, 's> {
    pub commands: &'a mut Commands<'w, 's>,
    pub asset_server: &'a AssetServer,
    pub skin: &'a ActiveNoteSkin,
    pub layout: &'a NoteField,
}

impl LaneEffects<'_, '_, '_> {
    /// The arrow flash at a receptor when a step's arrows vanish, growing
    /// while it fades. The bright variant plays at high combo: larger
    /// art, snappier, starting smaller.
    pub fn arrow_flash(
        &mut self,
        column: usize,
        target_y: f32,
        color: Color,
        bright: bool,
    ) -> Entity {
        let (flash, seconds, base_zoom, growth) = if bright {
            (&self.skin.flash_bright, 0.13, 0.8, 0.5)
        } else {
            (&self.skin.flash_dim, 0.18, 1.0, 0.4)
        };
        let size = flash.size * (self.layout.arrow_size / NOTE_CELL) * base_zoom;
        self.effect(
            flash.image.clone(),
            color,
            Transform {
                translation: Vec3::new(self.layout.column_x(column), target_y, 22.0),
                rotation: column_rotation(column),
                scale: size.extend(1.0),
            },
            FadeOut::growing(seconds, growth),
        )
    }

    pub fn mine_explosion(&mut self, column: usize, target_y: f32) -> Entity {
        self.effect(
            self.skin.mine_explosion.image.clone(),
            Color::WHITE,
            Transform {
                translation: Vec3::new(self.layout.column_x(column), target_y, 21.0),
                scale: Vec3::splat(self.layout.arrow_size * 1.7),
                ..default()
            },
            FadeOut::growing(MINE_EXPLOSION_SECONDS, 0.25),
        )
    }

    fn effect(
        &mut self,
        image: Handle<Image>,
        color: Color,
        transform: Transform,
        fade: FadeOut,
    ) -> Entity {
        let material = self.asset_server.add(effect_material(image, color));
        let visual = self.skin.quad_visual(material);
        self.commands
            .spawn((
                Mesh3d(visual.mesh),
                MeshMaterial3d(visual.material),
                self.layout.render_layers(),
                transform,
                fade,
                FadesMaterial,
            ))
            .id()
    }
}

/// The skin's arrows point down; rotate per pad-local column so every
/// group of four reads Left, Down, Up, Right (doubles repeats the cycle
/// on the second pad).
pub fn column_rotation(column: usize) -> Quat {
    let angle = match column % PAD_COLUMNS {
        0 => -std::f32::consts::FRAC_PI_2,
        1 => 0.0,
        2 => std::f32::consts::PI,
        _ => std::f32::consts::FRAC_PI_2,
    };
    Quat::from_rotation_z(angle)
}

/// The note-field animation systems, for consumers that order their state
/// updates relative to the rendering.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct NoteFieldSystems;

pub struct NoteFieldPlugin;

impl Plugin for NoteFieldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                place_field_elements,
                position_receptors,
                scroll_arrows,
                animate_sheet_taps,
                animate_hold_heads,
                animate_receptor_frames,
                animate_receptor_press,
                animate_hold_parts,
                animate_mines,
            )
                .chain()
                .in_set(NoteFieldSystems)
                .run_if(resource_exists::<NoteFieldClock>),
        )
        // Fades run even without a clock, so transients spawned during a
        // session always finish dying after it is torn down.
        .add_systems(
            Update,
            (fade_out.after(NoteFieldSystems), sync_lane_cameras),
        );
    }
}

/// Notes spawn far off-screen and are placed by [`scroll_arrows`] from their
/// first frame.
const OFFSCREEN_Y: f32 = -10_000.0;

fn spawn_element(
    commands: &mut Commands,
    visual: ElementVisual,
    layout: &NoteField,
    column: usize,
    y: f32,
    z: f32,
    rotation: Quat,
) -> Entity {
    let entity = commands
        .spawn((
            Mesh3d(visual.mesh),
            MeshMaterial3d(visual.material),
            layout.render_layers(),
            InColumn {
                column,
                cell: visual.cell,
            },
            Transform {
                translation: Vec3::new(layout.column_x(column), y, z),
                rotation,
                scale: Vec3::splat(layout.arrow_size / visual.cell),
            },
        ))
        .id();
    if let Some((mesh, material)) = visual.shell {
        // The nudge mirrors the mesh layering (the shell's front face sits
        // above the fill plate), so the shell also blend-sorts right after
        // its own fill under any camera.
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            layout.render_layers(),
            Transform::from_xyz(0.0, 0.0, 0.05),
            ChildOf(entity),
        ));
    }
    entity
}

/// A note field's own perspective camera, pivoting around the receptor row
/// per its player's [`Perspective`].
#[derive(Component)]
struct LaneCamera {
    field: Entity,
}

/// The lane cameras' projection: a perspective whose window is shifted
/// sideways, so a camera hovering over its own field still shows the same
/// screen rect as the main camera. Every field gets its own vanishing
/// point — off-center fields (versus lanes, the options previews) face
/// the viewer head on instead of leaning toward the screen center.
#[derive(Debug, Clone)]
struct LanePerspective {
    perspective: PerspectiveProjection,
    /// Horizontal window shift in NDC units (+1 is half a screen).
    shift: f32,
}

impl CameraProjection for LanePerspective {
    fn get_clip_from_view(&self) -> Mat4 {
        let mut matrix = self.perspective.get_clip_from_view();
        // The off-center window term of a frustum, `(right+left)/(right-left)`.
        matrix.col_mut(2)[0] = self.shift;
        matrix
    }

    fn get_clip_from_view_for_sub(&self, sub_view: &bevy::camera::SubCameraView) -> Mat4 {
        self.perspective.get_clip_from_view_for_sub(sub_view)
    }

    fn update(&mut self, width: f32, height: f32) {
        self.perspective.update(width, height);
    }

    fn far(&self) -> f32 {
        self.perspective.far()
    }

    fn get_frustum_corners(&self, z_near: f32, z_far: f32) -> [Vec3A; 8] {
        self.perspective.get_frustum_corners(z_near, z_far)
    }
}

/// Keeps one lane camera per note field: spawned when the field appears,
/// culled when it goes, and re-aimed every frame. The camera hovers over
/// its field's center where a flat view reproduces the 2D world 1:1 on
/// the lane plane, then pitches around the receptor row, so the receptors
/// stay put while the lane foreshortens.
fn sync_lane_cameras(
    mut commands: Commands,
    config: Res<GameConfig>,
    fields: Query<(Entity, &NoteField)>,
    settings: Res<PlayerSettings>,
    clock: Option<Res<NoteFieldClock>>,
    mut cameras: Query<(
        Entity,
        &LaneCamera,
        &Camera,
        &mut Transform,
        &mut Projection,
    )>,
) {
    let fov = config.lane_camera.fov_degrees.to_radians();
    for (entity, lane_camera, ..) in &cameras {
        if fields.get(lane_camera.field).is_err() {
            commands.entity(entity).despawn();
        }
    }
    let Some(clock) = clock else { return };
    for (field_entity, field) in &fields {
        let Some((_, _, camera, mut transform, mut projection)) = cameras
            .iter_mut()
            .find(|(_, lane_camera, ..)| lane_camera.field == field_entity)
        else {
            commands.spawn((
                LaneCamera {
                    field: field_entity,
                },
                Camera3d::default(),
                Camera {
                    order: (LANE_LAYER_BASE + field.lane) as isize,
                    clear_color: ClearColorConfig::None,
                    ..default()
                },
                field.view.target.clone(),
                Tonemapping::None,
                field.render_layers(),
                Projection::custom(LanePerspective {
                    perspective: PerspectiveProjection { fov, ..default() },
                    shift: 0.0,
                }),
            ));
            continue;
        };
        let Some(size) = camera.logical_target_size() else {
            continue;
        };
        let perspective = settings[field.player].perspective;

        // The world rect the main ortho camera shows, covering the canvas.
        let canvas = field.view.canvas;
        let visible = Vec2::new(
            size.x * (canvas.x / size.x).max(canvas.y / size.y),
            size.y * (canvas.x / size.x).max(canvas.y / size.y),
        );
        let distance = visible.y * 0.5 / (fov * 0.5).tan();
        let tilt_magnitude = config.lane_camera.tilt_degrees.to_radians();
        let tilt = match perspective {
            Perspective::None => 0.0,
            Perspective::Above => -tilt_magnitude,
            Perspective::Below => tilt_magnitude,
        };
        let mut wanted = Transform::from_xyz(field.origin_x, 0.0, distance);
        wanted.rotate_around(
            Vec3::new(field.origin_x, clock.target_y, 0.0),
            Quat::from_rotation_x(tilt),
        );
        if *transform != wanted {
            *transform = wanted;
        }
        if let Projection::Custom(custom) = &mut *projection
            && let Some(lane) = custom.get_mut::<LanePerspective>()
        {
            lane.perspective.near = distance * 0.05;
            lane.perspective.far = distance * 4.0;
            // Shift the projection window back over the screen rect the
            // main camera shows.
            lane.shift = -field.origin_x / (visible.x * 0.5);
        }
    }
}

/// Keeps every element's x and footprint derived from its field, so a
/// refit — the window rescaling the arrow pixel budget — moves whole
/// lanes without respawning them. The receptor press and hold-part
/// systems run later in the chain and refine the scales they own;
/// fading entities are left to die at their size.
fn place_field_elements(
    fields: Query<&NoteField>,
    mut elements: Query<(&InColumn, &InField, &mut Transform), Without<FadeOut>>,
) {
    for (anchor, in_field, mut transform) in &mut elements {
        let Ok(field) = fields.get(in_field.0) else {
            continue;
        };
        let x = field.column_x(anchor.column);
        if transform.translation.x != x {
            transform.translation.x = x;
        }
        let scale = Vec3::splat(field.arrow_size / anchor.cell);
        if transform.scale != scale {
            transform.scale = scale;
        }
    }
}

fn position_receptors(
    clock: Res<NoteFieldClock>,
    mut receptors: Query<&mut Transform, With<Receptor>>,
) {
    for mut transform in &mut receptors {
        if transform.translation.y != clock.target_y {
            transform.translation.y = clock.target_y;
        }
    }
}

/// Arrows scroll up from the bottom and meet their receptor exactly on time —
/// position is derived from the clock, never accumulated. The head of a
/// pinned hold sticks at the receptors until the hold resolves.
fn scroll_arrows(
    clock: Res<NoteFieldClock>,
    fields: Query<&NoteField>,
    mut arrows: Query<
        (&NoteArrow, &InField, Option<&HoldVisual>, &mut Transform),
        Without<FadeOut>,
    >,
) {
    let scroll = clock.scroll();
    for (arrow, in_field, hold, mut transform) in &mut arrows {
        let Ok(field) = fields.get(in_field.0) else {
            continue;
        };
        let mut y = scroll.y_at(field, arrow.time, arrow.beat);
        if hold.is_some_and(|hold| hold.state.pinned()) {
            y = y.min(scroll.target_y());
        }
        transform.translation.y = y;
    }
}

/// Slides every sheet skin's tap materials to the frame of the current
/// beat — all taps of a quant share one material, animating in unison.
fn animate_sheet_taps(
    clock: Res<NoteFieldClock>,
    skins: Res<ActiveNoteSkins>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let beat = clock.beat();
    for player in PlayerId::iter() {
        let NoteArt::Sheet(sheet) = &skins.get(player).note else {
            continue;
        };
        let x = sheet.frame_x_at(beat);
        for handle in sheet.tap_materials() {
            let Some(material) = materials.get(handle) else {
                continue;
            };
            if material.uv_transform.translation.x != x {
                materials
                    .get_mut(handle)
                    .expect("material existed just above")
                    .uv_transform
                    .translation
                    .x = x;
            }
        }
    }
}

fn animate_hold_heads(
    skins: Res<ActiveNoteSkins>,
    fields: Query<&NoteField>,
    mut heads: Query<(
        &HoldHead,
        &HoldVisual,
        &InField,
        &mut MeshMaterial3d<StandardMaterial>,
    )>,
) {
    for (head, hold, in_field, mut material) in &mut heads {
        let Ok(field) = fields.get(in_field.0) else {
            continue;
        };
        let skin = skins.get(field.player);
        let visual = skin.head_visual(head.row, hold.state.active());
        if material.0 != visual.material {
            material.0 = visual.material;
        }
    }
}

fn animate_receptor_frames(
    clock: Res<NoteFieldClock>,
    skins: Res<ActiveNoteSkins>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let beat = clock.beat();
    for player in PlayerId::iter() {
        let receptor = &skins.get(player).receptor;
        let x = receptor.frame_x_at(beat);
        let brightness = receptor.brightness_at(beat);
        let color = Color::srgb(brightness, brightness, brightness);
        let Some(material) = materials.get(&receptor.material) else {
            continue;
        };
        if material.uv_transform.translation.x != x || material.base_color != color {
            let mut material = materials
                .get_mut(&receptor.material)
                .expect("material existed just above");
            material.uv_transform.translation.x = x;
            material.base_color = color;
        }
    }
}

const PRESS_SECONDS: f32 = 0.25;

/// Held receptors tween back along Z with a shrink to sell the depth.
fn animate_receptor_press(
    time: Res<Time>,
    fields: Query<&NoteField>,
    mut receptors: Query<(&mut Receptor, &InColumn, &InField, &mut Transform)>,
) {
    for (mut receptor, anchor, in_field, mut transform) in &mut receptors {
        let Ok(field) = fields.get(in_field.0) else {
            continue;
        };
        if !receptor.held && receptor.press == 0.0 {
            continue;
        }
        let step = time.delta_secs() / PRESS_SECONDS;
        let step = if receptor.held { step } else { -step };
        receptor.press = (receptor.press + step).clamp(0.0, 1.0);
        let eased = EaseFunction::CubicInOut.sample_clamped(receptor.press);
        let base = field.arrow_size / anchor.cell;
        transform.translation.z = 10.0 - 6.0 * eased;
        transform.scale = Vec3::splat(base * (1.0 - 0.22 * eased));
    }
}

/// The body slides this far under the cap, so the cap's filtered top edge
/// blends into the body instead of the background.
const BODY_CAP_OVERLAP: f32 = 1.0;

/// Positions and styles the hold tail: the body is one quad whose texture
/// wraps vertically, anchored to the tail so the pattern always meets the
/// cap at a tile boundary, and the cap sits centered on the tail below it —
/// clipped so nothing draws above the head's center. Each part owns its
/// material: the texture switches between active and inactive with the
/// hold's state, the texture window drives the tiling and clipping, and
/// dropped holds dim to the skin's NG brightness.
#[derive(QueryData)]
#[query_data(mutable)]
struct HoldPartVisual {
    part: &'static HoldPart,
    transform: &'static mut Transform,
    material: &'static MeshMaterial3d<StandardMaterial>,
    visibility: &'static mut Visibility,
}

fn animate_hold_parts(
    clock: Res<NoteFieldClock>,
    skins: Res<ActiveNoteSkins>,
    fields: Query<&NoteField>,
    heads: Query<(&NoteArrow, &HoldVisual, &InField)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut parts: Query<HoldPartVisual, (Without<NoteArrow>, Without<FadeOut>)>,
) {
    let scroll = clock.scroll();
    for item in &mut parts {
        let HoldPartVisualItem {
            part,
            mut transform,
            material,
            mut visibility,
        } = item;
        let Ok((arrow, hold, in_field)) = heads.get(part.head) else {
            continue;
        };
        let Ok(field) = fields.get(in_field.0) else {
            continue;
        };
        let skin = skins.get(field.player);
        let art = if part.roll { &skin.roll } else { &skin.hold };
        let scale = field.arrow_size / art.body_size.x;
        let cap_height = art.cap_size.y * scale;
        if hold.state == HoldVisualState::Ok {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        }

        let mut head_y = scroll.y_at(field, arrow.time, arrow.beat);
        if hold.state.pinned() {
            head_y = head_y.min(scroll.target_y());
        }
        let end_y = scroll.y_at(field, hold.end, hold.end_beat);
        let body_bottom = end_y + art.stop_above_tail * scale;

        let active = hold.state.active();
        let brightness = if hold.state == HoldVisualState::Dropped {
            skin.dropped_brightness
        } else {
            1.0
        };
        let color = Color::srgb(brightness, brightness, brightness);

        match part.piece {
            HoldPiece::Body => {
                let length = head_y - body_bottom;
                if length <= 0.5 {
                    visibility.set_if_neq(Visibility::Hidden);
                    continue;
                }
                let height = length + BODY_CAP_OVERLAP;
                // The texture window ends on the texture's bottom edge so
                // the pattern is anchored to the tail, whatever the length;
                // the wrap sampler tiles it upward from there.
                let top = art.body_size.y - length / scale;
                let bottom = art.body_size.y + BODY_CAP_OVERLAP / scale;
                let window = uv_window(top, bottom, art.body_size.y);
                let texture = if active {
                    &art.body_active
                } else {
                    &art.body_inactive
                };
                update_part_material(&mut materials, &material.0, texture, window, color);
                transform.translation.y = head_y - height / 2.0;
                transform.scale = Vec3::new(field.arrow_size, height, 1.0);
                visibility.set_if_neq(Visibility::Visible);
            }
            HoldPiece::Cap => {
                // Clipped at the head's center; the bottom of the texture
                // stays, so the tail keeps its tip.
                let top = body_bottom.min(head_y);
                let bottom = body_bottom - cap_height;
                let visible = (top - bottom).min(cap_height);
                if visible <= 0.5 {
                    visibility.set_if_neq(Visibility::Hidden);
                    continue;
                }
                let hidden = (art.cap_size.y - visible / scale).max(0.0);
                let window = uv_window(hidden, art.cap_size.y, art.cap_size.y);
                let texture = if active {
                    &art.cap_active
                } else {
                    &art.cap_inactive
                };
                update_part_material(&mut materials, &material.0, texture, window, color);
                transform.translation.y = bottom + visible / 2.0;
                transform.scale = Vec3::new(field.arrow_size, visible, 1.0);
                visibility.set_if_neq(Visibility::Visible);
            }
        }
    }
}

/// The vertical texture window `top..bottom` (in texture pixels of a
/// `texture_height` tall image) as a texture-coordinate transform for a
/// unit quad.
fn uv_window(top: f32, bottom: f32, texture_height: f32) -> Affine2 {
    Affine2 {
        matrix2: Mat2::from_diagonal(Vec2::new(1.0, (bottom - top) / texture_height)),
        translation: Vec2::new(0.0, top / texture_height),
    }
}

fn update_part_material(
    materials: &mut Assets<StandardMaterial>,
    handle: &Handle<StandardMaterial>,
    texture: &Handle<Image>,
    window: Affine2,
    color: Color,
) {
    let Some(material) = materials.get(handle) else {
        return;
    };
    if material.base_color_texture.as_ref() == Some(texture)
        && material.uv_transform == window
        && material.base_color == color
    {
        return;
    }
    let mut material = materials
        .get_mut(handle)
        .expect("material existed just above");
    material.base_color_texture = Some(texture.clone());
    material.uv_transform = window;
    material.base_color = color;
}

fn animate_mines(
    clock: Res<NoteFieldClock>,
    skins: Res<ActiveNoteSkins>,
    fields: Query<&NoteField>,
    mut mines: Query<(&MineNote, &InField, &mut Transform), Without<FadeOut>>,
) {
    let scroll = clock.scroll();
    let beat = clock.beat();
    for (mine, in_field, mut transform) in &mut mines {
        let Ok(field) = fields.get(in_field.0) else {
            continue;
        };
        let skin = skins.get(field.player);
        transform.translation.y = scroll.y_at(field, mine.time, mine.beat);
        transform.rotation = Quat::from_rotation_z(
            (-(beat.0.rem_euclid(skin.mine_spin_beats) / skin.mine_spin_beats)
                * std::f64::consts::TAU) as f32,
        );
    }
}

#[derive(QueryData)]
#[query_data(mutable)]
struct FadingVisual {
    entity: Entity,
    fade: &'static mut FadeOut,
    transform: &'static mut Transform,
    text_color: Option<&'static mut TextColor>,
    sprite: Option<&'static mut Sprite>,
    material: Option<&'static MeshMaterial3d<StandardMaterial>>,
    fades_material: Has<FadesMaterial>,
}

fn fade_out(
    time: Res<Time>,
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut fading: Query<FadingVisual>,
) {
    for item in &mut fading {
        let FadingVisualItem {
            entity,
            mut fade,
            mut transform,
            text_color,
            sprite,
            material,
            fades_material,
        } = item;
        fade.remaining -= time.delta_secs();
        if fade.remaining <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let alpha = fade.remaining / fade.total;
        if fade.growth != 0.0 {
            let base = *fade.base_scale.get_or_insert(transform.scale);
            transform.scale = base * (1.0 + fade.growth * (1.0 - alpha));
        }
        if let Some(mut color) = text_color {
            color.0.set_alpha(alpha);
        }
        if let Some(mut sprite) = sprite {
            sprite.color = sprite.color.with_alpha(alpha);
        }
        if fades_material
            && let Some(handle) = material
            && let Some(mut material) = materials.get_mut(&handle.0)
        {
            material.base_color.set_alpha(alpha);
        }
    }
}
