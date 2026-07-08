use crate::core::note_skin::ActiveNoteSkin;
use crate::core::stepfile::StepfileTiming;
use crate::core::units::{Beat, Seconds};
use crate::core::{at, oriented};
use bevy::ecs::query::QueryData;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub const TARGET_Y: f32 = 260.0;
pub const COLUMN_SPACING: f32 = 100.0;
pub const ARROW_SIZE: f32 = 88.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum NoteSpeed {
    /// A constant rate regardless of the chart's tempo, expressed as the
    /// scroll BPM at which [`NoteSpeed::Dynamic`] would move equally fast.
    Constant(f32),
    /// Spacing follows the chart's beats — one arrow height per beat at
    /// multiplier 1 — so BPM changes stretch the scroll and stops freeze it.
    Dynamic(f32),
}

impl NoteSpeed {
    pub fn value(self) -> f32 {
        match self {
            NoteSpeed::Constant(value) | NoteSpeed::Dynamic(value) => value,
        }
    }
}

/// Paces every note-field animation: `visible` is the current moment on the
/// drawn timeline and `timing` converts it to beats. The field's systems run
/// only while this resource exists. The owner of the field inserts it,
/// advances `visible`, and flips the state components ([`Receptor::held`],
/// [`HoldVisual`], [`FadeOut`]) — gameplay rules stay with the owner.
#[derive(Resource)]
pub struct NoteFieldClock {
    pub visible: Seconds,
    pub timing: StepfileTiming,
    pub speed: NoteSpeed,
}

impl NoteFieldClock {
    pub fn beat(&self) -> f64 {
        self.timing.beat_at_seconds(self.visible).0
    }

    pub fn scroll(&self) -> NoteScroll {
        NoteScroll {
            now: self.visible,
            now_beat: self.beat(),
            speed: self.speed,
        }
    }
}

/// A per-frame snapshot placing notes on screen: [`NoteSpeed::Constant`]
/// spaces notes by their seconds, [`NoteSpeed::Dynamic`] by their beats.
pub struct NoteScroll {
    now: Seconds,
    now_beat: f64,
    speed: NoteSpeed,
}

impl NoteScroll {
    pub fn y_at(&self, time: Seconds, beat: Beat) -> f32 {
        let arrows_until = match self.speed {
            NoteSpeed::Constant(scroll_bpm) => (time - self.now).0 * scroll_bpm as f64 / 60.0,
            NoteSpeed::Dynamic(multiplier) => (beat.0 - self.now_beat) * multiplier as f64,
        };
        TARGET_Y - (arrows_until * ARROW_SIZE as f64) as f32
    }
}

#[derive(Component, Default, Clone)]
pub struct Receptor {
    pub column: usize,
    /// The press tween follows this.
    pub held: bool,
    press: f32,
}

#[derive(Component, Clone, FromTemplate)]
pub struct NoteArrow {
    pub time: Seconds,
    pub beat: Beat,
}

/// An arrow cycling through its quant row's animation frames.
#[derive(Component, Clone, FromTemplate)]
pub struct TapAnimation {
    pub first_frame: usize,
}

/// An arrow drawn as the skin's hold head, switching with the hold's state.
#[derive(Component, Clone, FromTemplate)]
pub struct HoldHeadSprite {
    pub skin_row: usize,
}

/// Render state of a hold, on the same entity as its head arrow.
#[derive(Component, Clone, FromTemplate)]
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

#[derive(Component, Clone, FromTemplate)]
pub struct HoldPart {
    pub head: Entity,
    pub piece: HoldPiece,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromTemplate)]
pub enum HoldPiece {
    #[default]
    Body,
    Cap,
}

impl From<HoldPiece> for HoldPieceTemplate {
    fn from(piece: HoldPiece) -> Self {
        match piece {
            HoldPiece::Body => HoldPieceTemplate::Body,
            HoldPiece::Cap => HoldPieceTemplate::Cap,
        }
    }
}

#[derive(Component, Clone, FromTemplate)]
pub struct MineNote {
    pub time: Seconds,
    pub beat: Beat,
}

pub const GRADED_FADE_SECONDS: f32 = 0.05;
const MINE_EXPLOSION_SECONDS: f32 = 0.4;

/// Fades the entity out where it stands, then despawns it; fading arrows
/// stop scrolling because [`scroll_arrows`] skips them.
#[derive(Component)]
pub struct FadeOut {
    remaining: f32,
    total: f32,
    growth: f32,
}

impl FadeOut {
    pub fn over(seconds: f32) -> FadeOut {
        FadeOut {
            remaining: seconds,
            total: seconds,
            growth: 0.0,
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
    /// Hold or roll tail position.
    pub end: Option<(Seconds, Beat)>,
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

pub fn spawn_receptors(commands: &mut Commands, skin: &ActiveNoteSkin) -> [Entity; 4] {
    std::array::from_fn(|column| {
        commands
            .spawn_scene(bsn! {
                Receptor { column: column }
                skin_sprite(skin, skin.receptor_frames[1], ARROW_SIZE)
                oriented(column_x(column), TARGET_Y, 10.0, column_rotation(column))
            })
            .id()
    })
}

pub fn spawn_note(commands: &mut Commands, skin: &ActiveNoteSkin, note: &NoteSpawn) -> SpawnedNote {
    let row = skin.quant_row(note.quant);
    let time = note.time;
    let beat = note.beat;
    let translation = Vec3::new(column_x(note.column), OFFSCREEN_Y, 20.0);
    let rotation = column_rotation(note.column);
    let head = match note.end {
        None => {
            let first_frame = skin.tap_base(row);
            commands
                .spawn_scene(bsn! {
                    NoteArrow { time: time, beat: beat }
                    TapAnimation { first_frame: first_frame }
                    skin_sprite(skin, first_frame, ARROW_SIZE)
                    oriented(translation.x, translation.y, translation.z, rotation)
                })
                .id()
        }
        Some((end, end_beat)) => commands
            .spawn_scene(bsn! {
                NoteArrow { time: time, beat: beat }
                HoldHeadSprite { skin_row: row }
                HoldVisual { end: {end}, end_beat: {end_beat} }
                skin_sprite(skin, skin.hold_head(row, false), ARROW_SIZE)
                oriented(translation.x, translation.y, translation.z, rotation)
            })
            .id(),
    };

    let mut parts = Vec::new();
    if note.end.is_some() {
        let x = column_x(note.column);
        let body = skin.hold_body_inactive.clone();
        parts.push(
            commands
                .spawn_scene(bsn! {
                    HoldPart { head: {head}, piece: HoldPiece::Body }
                    Sprite { image: {body}, custom_size: {Some(Vec2::ZERO)} }
                    at(x, OFFSCREEN_Y, 18.0)
                })
                .id(),
        );
        parts.push(
            commands
                .spawn_scene(bsn! {
                    HoldPart { head: {head}, piece: HoldPiece::Cap }
                    skin_sprite(skin, skin.hold_cap_inactive, ARROW_SIZE)
                    at(x, OFFSCREEN_Y, 18.2)
                })
                .id(),
        );
    }

    SpawnedNote { head, parts }
}

pub fn spawn_mine(
    commands: &mut Commands,
    skin: &ActiveNoteSkin,
    time: Seconds,
    beat: Beat,
    column: usize,
) -> Entity {
    commands
        .spawn_scene(bsn! {
            MineNote { time: time, beat: beat }
            skin_sprite(skin, skin.mine, ARROW_SIZE)
            at(column_x(column), OFFSCREEN_Y, 20.0)
        })
        .id()
}

/// The arrow flash at a receptor when a step's arrows vanish, growing
/// while it fades. The bright variant plays at high combo: larger art,
/// snappier, starting smaller.
pub fn spawn_arrow_flash(
    commands: &mut Commands,
    skin: &ActiveNoteSkin,
    column: usize,
    color: Color,
    bright: bool,
) -> Entity {
    let (flash, seconds, base_zoom, growth) = if bright {
        (skin.arrow_flash_bright, 0.13, 0.8, 0.5)
    } else {
        (skin.arrow_flash_dim, 0.18, 1.0, 0.4)
    };
    let size = flash.size * (ARROW_SIZE / 64.0) * base_zoom;
    commands
        .spawn_scene(bsn! {
            skin_sprite(skin, flash.frame, size)
            Sprite { color: {color} }
            oriented(column_x(column), TARGET_Y, 22.0, column_rotation(column))
        })
        .insert(FadeOut::growing(seconds, growth))
        .id()
}

pub fn spawn_mine_explosion(
    commands: &mut Commands,
    skin: &ActiveNoteSkin,
    column: usize,
) -> Entity {
    commands
        .spawn_scene(bsn! {
            skin_sprite(skin, skin.mine_explosion, ARROW_SIZE * 1.7)
            at(column_x(column), TARGET_Y, 21.0)
        })
        .insert(FadeOut::growing(MINE_EXPLOSION_SECONDS, 0.25))
        .id()
}

pub fn skin_sprite(skin: &ActiveNoteSkin, index: usize, size: f32) -> impl Scene {
    let image = skin.sheet.clone();
    let rect = Some(skin.frame(index));
    bsn! {
        Sprite {
            image: {image},
            rect: {rect},
            custom_size: {Some(Vec2::splat(size))},
        }
    }
}

pub fn column_x(column: usize) -> f32 {
    (column as f32 - 1.5) * COLUMN_SPACING
}

/// The skin's sprites point down; rotate per column (Left, Down, Up, Right).
pub fn column_rotation(column: usize) -> Quat {
    let angle = match column {
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
                scroll_arrows,
                animate_tap_frames,
                animate_hold_heads,
                animate_receptor_frames,
                animate_receptor_press,
                animate_hold_parts,
                animate_mines,
                fade_out,
            )
                .chain()
                .in_set(NoteFieldSystems)
                .run_if(resource_exists::<NoteFieldClock>),
        );
    }
}

/// Notes spawn far off-screen and are placed by [`scroll_arrows`] from their
/// first frame.
const OFFSCREEN_Y: f32 = -10_000.0;

/// Arrows scroll up from the bottom and meet their receptor exactly on time —
/// position is derived from the clock, never accumulated. The head of a
/// pinned hold sticks at the receptors until the hold resolves.
fn scroll_arrows(
    clock: Res<NoteFieldClock>,
    mut arrows: Query<(&NoteArrow, Option<&HoldVisual>, &mut Transform), Without<FadeOut>>,
) {
    let scroll = clock.scroll();
    for (arrow, hold, mut transform) in &mut arrows {
        let mut y = scroll.y_at(arrow.time, arrow.beat);
        if hold.is_some_and(|hold| hold.state.pinned()) {
            y = y.min(TARGET_Y);
        }
        transform.translation.y = y;
    }
}

fn animate_tap_frames(
    clock: Res<NoteFieldClock>,
    skin: Res<ActiveNoteSkin>,
    mut taps: Query<(&TapAnimation, &mut Sprite)>,
) {
    let cycle = clock.beat().rem_euclid(skin.tap_beats_per_cycle) / skin.tap_beats_per_cycle;
    let frame = ((cycle * skin.tap_frames as f64) as usize).min(skin.tap_frames - 1);
    for (tap, mut sprite) in &mut taps {
        set_rect(&mut sprite, Some(skin.frame(tap.first_frame + frame)));
    }
}

fn animate_hold_heads(
    skin: Res<ActiveNoteSkin>,
    mut heads: Query<(&HoldHeadSprite, &HoldVisual, &mut Sprite)>,
) {
    for (head, hold, mut sprite) in &mut heads {
        let index = skin.hold_head(head.skin_row, hold.state.active());
        set_rect(&mut sprite, Some(skin.frame(index)));
    }
}

fn animate_receptor_frames(
    clock: Res<NoteFieldClock>,
    skin: Res<ActiveNoteSkin>,
    mut receptors: Query<&mut Sprite, With<Receptor>>,
) {
    let receptor_frame = if clock.beat().rem_euclid(1.0) < skin.receptor_beat_split {
        skin.receptor_frames[0]
    } else {
        skin.receptor_frames[1]
    };
    for mut sprite in &mut receptors {
        set_rect(&mut sprite, Some(skin.frame(receptor_frame)));
    }
}

const PRESS_SECONDS: f32 = 0.25;

/// Held receptors tween back along Z with a shrink to sell the depth.
fn animate_receptor_press(time: Res<Time>, mut receptors: Query<(&mut Receptor, &mut Transform)>) {
    for (mut receptor, mut transform) in &mut receptors {
        if !receptor.held && receptor.press == 0.0 {
            continue;
        }
        let step = time.delta_secs() / PRESS_SECONDS;
        let step = if receptor.held { step } else { -step };
        receptor.press = (receptor.press + step).clamp(0.0, 1.0);
        let eased = EaseFunction::CubicInOut.sample_clamped(receptor.press);
        transform.translation.z = 10.0 - 6.0 * eased;
        transform.scale = Vec3::splat(1.0 - 0.22 * eased);
    }
}

/// The body slides this far under the cap, so the cap's filtered top edge
/// blends into the body instead of the background.
const BODY_CAP_OVERLAP: f32 = 1.0;

/// Positions and styles the hold tail: the body is one quad whose texture
/// wraps vertically, anchored to the tail so the pattern always meets the
/// cap at a tile boundary, and the cap sits centered on the tail below it —
/// clipped so nothing draws above the head's center. Textures switch
/// between active and inactive with the hold's state, and dropped holds dim
/// to the skin's NG brightness.
#[derive(QueryData)]
#[query_data(mutable)]
struct HoldPartSprite {
    part: &'static HoldPart,
    transform: &'static mut Transform,
    sprite: &'static mut Sprite,
    visibility: &'static mut Visibility,
}

fn animate_hold_parts(
    clock: Res<NoteFieldClock>,
    skin: Res<ActiveNoteSkin>,
    heads: Query<(&NoteArrow, &HoldVisual)>,
    mut parts: Query<HoldPartSprite, Without<NoteArrow>>,
) {
    let scroll = clock.scroll();
    let scale = ARROW_SIZE / skin.hold_body_size.x;
    let cap_height = skin.hold_cap_size.y * scale;
    for item in &mut parts {
        let HoldPartSpriteItem {
            part,
            mut transform,
            mut sprite,
            mut visibility,
        } = item;
        let Ok((arrow, hold)) = heads.get(part.head) else {
            continue;
        };
        if hold.state == HoldVisualState::Ok {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        }

        let mut head_y = scroll.y_at(arrow.time, arrow.beat);
        if hold.state.pinned() {
            head_y = head_y.min(TARGET_Y);
        }
        let end_y = scroll.y_at(hold.end, hold.end_beat);
        let body_bottom = end_y + skin.hold_body_stop_above_tail * scale;

        let active = hold.state.active();
        let brightness = if hold.state == HoldVisualState::Dropped {
            skin.dropped_brightness
        } else {
            1.0
        };
        let color = Color::srgb(brightness, brightness, brightness);
        if sprite.color != color {
            sprite.color = color;
        }

        match part.piece {
            HoldPiece::Body => {
                let image = if active {
                    &skin.hold_body_active
                } else {
                    &skin.hold_body_inactive
                };
                if sprite.image != *image {
                    sprite.image = image.clone();
                }
                let length = head_y - body_bottom;
                if length <= 0.5 {
                    visibility.set_if_neq(Visibility::Hidden);
                    continue;
                }
                // One quad; the repeat sampler wraps the pattern. The rect
                // ends on the texture's bottom edge so the pattern is
                // anchored to the tail, whatever the length.
                let height = length + BODY_CAP_OVERLAP;
                set_rect(
                    &mut sprite,
                    Some(Rect::new(
                        0.0,
                        skin.hold_body_size.y - length / scale,
                        skin.hold_body_size.x,
                        skin.hold_body_size.y + BODY_CAP_OVERLAP / scale,
                    )),
                );
                sprite.custom_size = Some(Vec2::new(ARROW_SIZE, height));
                transform.translation.y = head_y - height / 2.0;
                visibility.set_if_neq(Visibility::Visible);
            }
            HoldPiece::Cap => {
                let index = if active {
                    skin.hold_cap_active
                } else {
                    skin.hold_cap_inactive
                };
                let frame = skin.frame(index);
                // Clipped at the head's center; the bottom of the texture
                // stays, so the tail keeps its tip.
                let top = body_bottom.min(head_y);
                let bottom = body_bottom - cap_height;
                let visible = (top - bottom).min(cap_height);
                if visible <= 0.5 {
                    visibility.set_if_neq(Visibility::Hidden);
                    continue;
                }
                let rect = Rect::new(
                    frame.min.x,
                    frame.min.y + (skin.hold_cap_size.y - visible / scale).max(0.0),
                    frame.max.x,
                    frame.max.y,
                );
                set_rect(&mut sprite, Some(rect));
                sprite.custom_size = Some(Vec2::new(ARROW_SIZE, visible));
                transform.translation.y = bottom + visible / 2.0;
                visibility.set_if_neq(Visibility::Visible);
            }
        }
    }
}

fn animate_mines(
    clock: Res<NoteFieldClock>,
    skin: Res<ActiveNoteSkin>,
    mut mines: Query<(&MineNote, &mut Transform)>,
) {
    let scroll = clock.scroll();
    let beat = clock.beat();
    let spin = Quat::from_rotation_z(
        (-(beat.rem_euclid(skin.mine_spin_beats) / skin.mine_spin_beats) * std::f64::consts::TAU)
            as f32,
    );
    for (mine, mut transform) in &mut mines {
        transform.translation.y = scroll.y_at(mine.time, mine.beat);
        transform.rotation = spin;
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
}

fn fade_out(time: Res<Time>, mut commands: Commands, mut fading: Query<FadingVisual>) {
    for item in &mut fading {
        let FadingVisualItem {
            entity,
            mut fade,
            mut transform,
            text_color,
            sprite,
        } = item;
        fade.remaining -= time.delta_secs();
        if fade.remaining <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let alpha = fade.remaining / fade.total;
        if fade.growth != 0.0 {
            transform.scale = Vec3::splat(1.0 + fade.growth * (1.0 - alpha));
        }
        if let Some(mut color) = text_color {
            color.0.set_alpha(alpha);
        }
        if let Some(mut sprite) = sprite {
            sprite.color = sprite.color.with_alpha(alpha);
        }
    }
}

// The setters below assign only on change, so unchanged entities don't get
// flagged for re-extraction every frame; `set_if_neq` covers `Visibility`.

fn set_rect(sprite: &mut Mut<Sprite>, rect: Option<Rect>) {
    if sprite.rect != rect {
        sprite.rect = rect;
    }
}
