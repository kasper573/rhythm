use crate::core::note_skin::ActiveNoteSkin;
use crate::core::stepfile::StepfileTiming;
use crate::core::units::{Beat, Seconds};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub const TARGET_Y: f32 = 260.0;
pub const COLUMN_SPACING: f32 = 100.0;
pub const ARROW_SIZE: f32 = 88.0;

/// How arrows travel.
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
/// [`HoldVisual`], [`ArrowFade`]) — gameplay rules stay with the owner.
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

    pub fn scroll(&self) -> Scroll {
        Scroll {
            now: self.visible,
            now_beat: self.beat(),
            speed: self.speed,
        }
    }
}

/// A per-frame snapshot placing notes on screen: [`NoteSpeed::Constant`]
/// spaces notes by their seconds, [`NoteSpeed::Dynamic`] by their beats.
pub struct Scroll {
    now: Seconds,
    now_beat: f64,
    speed: NoteSpeed,
}

impl Scroll {
    pub fn y_at(&self, time: Seconds, beat: Beat) -> f32 {
        let arrows_until = match self.speed {
            NoteSpeed::Constant(scroll_bpm) => (time - self.now).0 * scroll_bpm as f64 / 60.0,
            NoteSpeed::Dynamic(multiplier) => (beat.0 - self.now_beat) * multiplier as f64,
        };
        TARGET_Y - (arrows_until * ARROW_SIZE as f64) as f32
    }
}

/// One of the four step receptors.
#[derive(Component)]
pub struct Receptor {
    pub column: usize,
    /// Whether the panel is pressed; the press tween follows this.
    pub held: bool,
    press: f32,
}

/// A tap arrow or hold head scrolling toward its receptor.
#[derive(Component)]
pub struct NoteArrow {
    pub time: Seconds,
    pub beat: Beat,
}

/// What a note arrow looks like: an animated quant-colored tap, or the
/// skin's hold head for that quant row.
#[derive(Component)]
pub enum ArrowVisual {
    Tap { base: usize },
    HoldHead { row: usize },
}

/// Render state of a hold, on the same entity as its head arrow.
#[derive(Component)]
pub struct HoldVisual {
    pub end: Seconds,
    pub end_beat: Beat,
    pub state: HoldVisualState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldVisualState {
    /// Not yet stepped: scrolls by whole, inactive textures.
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

/// One piece of a hold's tail, linked to its head arrow entity.
#[derive(Component)]
pub struct HoldPart {
    pub head: Entity,
    pub piece: HoldPiece,
}

/// The hold tail: the body pattern is anchored to the tail so it always
/// meets the cap at a tile boundary, and the cap is centered on the tail
/// below the body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldPiece {
    Body,
    Cap,
}

#[derive(Component)]
pub struct MineNote {
    pub time: Seconds,
    pub beat: Beat,
}

/// Fades a sprite out where it stands, then despawns it.
#[derive(Component)]
pub struct ArrowFade {
    pub remaining: f32,
    pub total: f32,
}

impl ArrowFade {
    pub fn over(seconds: f32) -> ArrowFade {
        ArrowFade {
            remaining: seconds,
            total: seconds,
        }
    }
}

/// A briefly visible effect that grows slightly and fades away.
#[derive(Component)]
pub struct Popup {
    pub remaining: f32,
    pub total: f32,
}

impl Popup {
    pub fn over(seconds: f32) -> Popup {
        Popup {
            remaining: seconds,
            total: seconds,
        }
    }
}

/// Everything the field needs to spawn one steppable note.
pub struct NoteSpawn {
    pub time: Seconds,
    pub beat: Beat,
    pub column: usize,
    /// Recognized quantization (see `GameConfig::recognized_quant`).
    pub quant: u32,
    /// Hold or roll tail position.
    pub end: Option<(Seconds, Beat)>,
}

/// The entities spawned for one note, for the caller to tag and track.
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
            .spawn((
                Receptor {
                    column,
                    held: false,
                    press: 0.0,
                },
                skin_sprite(skin, skin.receptor_frames[1], ARROW_SIZE),
                Transform::from_xyz(column_x(column), TARGET_Y, 10.0)
                    .with_rotation(column_rotation(column)),
            ))
            .id()
    })
}

pub fn spawn_note(commands: &mut Commands, skin: &ActiveNoteSkin, note: &NoteSpawn) -> SpawnedNote {
    let row = skin.quant_row(note.quant);
    let visual = match note.end {
        Some(_) => ArrowVisual::HoldHead { row },
        None => ArrowVisual::Tap {
            base: skin.tap_base(row),
        },
    };
    let sprite_index = match visual {
        ArrowVisual::Tap { base } => base,
        ArrowVisual::HoldHead { row } => skin.hold_head(row, false),
    };
    let mut head = commands.spawn((
        NoteArrow {
            time: note.time,
            beat: note.beat,
        },
        visual,
        skin_sprite(skin, sprite_index, ARROW_SIZE),
        Transform::from_xyz(column_x(note.column), OFFSCREEN_Y, 20.0)
            .with_rotation(column_rotation(note.column)),
    ));
    if let Some((end, end_beat)) = note.end {
        head.insert(HoldVisual {
            end,
            end_beat,
            state: HoldVisualState::Pending,
        });
    }
    let head = head.id();

    let mut parts = Vec::new();
    if note.end.is_some() {
        let x = column_x(note.column);
        parts.push(
            commands
                .spawn((
                    HoldPart {
                        head,
                        piece: HoldPiece::Body,
                    },
                    Sprite {
                        image: skin.hold_body_inactive.clone(),
                        custom_size: Some(Vec2::ZERO),
                        ..default()
                    },
                    Transform::from_xyz(x, OFFSCREEN_Y, 18.0),
                ))
                .id(),
        );
        parts.push(
            commands
                .spawn((
                    HoldPart {
                        head,
                        piece: HoldPiece::Cap,
                    },
                    skin_sprite(skin, skin.hold_cap_inactive, ARROW_SIZE),
                    Transform::from_xyz(x, OFFSCREEN_Y, 18.2),
                ))
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
        .spawn((
            MineNote { time, beat },
            skin_sprite(skin, skin.mine, ARROW_SIZE),
            Transform::from_xyz(column_x(column), OFFSCREEN_Y, 20.0),
        ))
        .id()
}

pub fn spawn_mine_explosion(
    commands: &mut Commands,
    skin: &ActiveNoteSkin,
    column: usize,
) -> Entity {
    commands
        .spawn((
            Popup::over(0.4),
            skin_sprite(skin, skin.mine_explosion, ARROW_SIZE * 1.7),
            Transform::from_xyz(column_x(column), TARGET_Y, 21.0),
        ))
        .id()
}

pub fn skin_sprite(skin: &ActiveNoteSkin, index: usize, size: f32) -> Sprite {
    Sprite {
        image: skin.sheet.clone(),
        texture_atlas: Some(TextureAtlas {
            layout: skin.layout.clone(),
            index,
        }),
        custom_size: Some(Vec2::splat(size)),
        ..default()
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
                animate_skin_frames,
                animate_receptor_press,
                animate_hold_parts,
                animate_mines,
                fade_graded_arrows,
                fade_popups,
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
    mut arrows: Query<(&NoteArrow, Option<&HoldVisual>, &mut Transform), Without<ArrowFade>>,
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

/// Drives the skin's animations: tap notes cycle their quant row over the
/// skin's beat cycle, receptors flash their two frames within each beat.
fn animate_skin_frames(
    clock: Res<NoteFieldClock>,
    skin: Res<ActiveNoteSkin>,
    mut arrows: Query<(&ArrowVisual, Option<&HoldVisual>, &mut Sprite)>,
    mut receptors: Query<&mut Sprite, (With<Receptor>, Without<ArrowVisual>)>,
) {
    let beat = clock.beat();
    let cycle = beat.rem_euclid(skin.tap_beats_per_cycle) / skin.tap_beats_per_cycle;
    let frame = ((cycle * skin.tap_frames as f64) as usize).min(skin.tap_frames - 1);
    for (visual, hold, mut sprite) in &mut arrows {
        let index = match visual {
            ArrowVisual::Tap { base } => base + frame,
            ArrowVisual::HoldHead { row } => {
                let active = hold.is_some_and(|hold| hold.state.active());
                skin.hold_head(*row, active)
            }
        };
        set_atlas_index(&mut sprite, index);
    }

    let receptor_frame = if beat.rem_euclid(1.0) < skin.receptor_beat_split {
        skin.receptor_frames[0]
    } else {
        skin.receptor_frames[1]
    };
    for mut sprite in &mut receptors {
        set_atlas_index(&mut sprite, receptor_frame);
    }
}

const PRESS_SECONDS: f32 = 0.25;

/// Held receptors tween "into the screen": back along Z with a shrink to
/// sell the depth, cubic-bezier eased, 250ms each way.
fn animate_receptor_press(time: Res<Time>, mut receptors: Query<(&mut Receptor, &mut Transform)>) {
    for (mut receptor, mut transform) in &mut receptors {
        if !receptor.held && receptor.press == 0.0 {
            continue;
        }
        let step = time.delta_secs() / PRESS_SECONDS;
        let step = if receptor.held { step } else { -step };
        receptor.press = (receptor.press + step).clamp(0.0, 1.0);
        let eased = ease_in_out_cubic(receptor.press);
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
#[allow(clippy::type_complexity)]
fn animate_hold_parts(
    clock: Res<NoteFieldClock>,
    skin: Res<ActiveNoteSkin>,
    heads: Query<(&NoteArrow, &HoldVisual)>,
    mut parts: Query<(&HoldPart, &mut Transform, &mut Sprite, &mut Visibility), Without<NoteArrow>>,
) {
    let scroll = clock.scroll();
    let scale = ARROW_SIZE / skin.hold_body_size.x;
    let cap_height = skin.hold_cap_size.y * scale;
    for (part, mut transform, mut sprite, mut visibility) in &mut parts {
        let Ok((arrow, hold)) = heads.get(part.head) else {
            continue;
        };
        if hold.state == HoldVisualState::Ok {
            set_visibility(&mut visibility, Visibility::Hidden);
            continue;
        }

        let mut head_y = scroll.y_at(arrow.time, arrow.beat);
        if hold.state.pinned() {
            head_y = head_y.min(TARGET_Y);
        }
        let end_y = scroll.y_at(hold.end, hold.end_beat);
        // The body runs from the head's center to a bit above the tail,
        // where the cap takes over.
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
                    set_visibility(&mut visibility, Visibility::Hidden);
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
                set_visibility(&mut visibility, Visibility::Visible);
            }
            HoldPiece::Cap => {
                let index = if active {
                    skin.hold_cap_active
                } else {
                    skin.hold_cap_inactive
                };
                set_atlas_index(&mut sprite, index);
                // Clipped at the head's center; the bottom of the texture
                // stays, so the tail keeps its tip.
                let top = body_bottom.min(head_y);
                let bottom = body_bottom - cap_height;
                let visible = (top - bottom).min(cap_height);
                if visible <= 0.5 {
                    set_visibility(&mut visibility, Visibility::Hidden);
                    continue;
                }
                let rect = (visible < cap_height).then(|| {
                    Rect::new(
                        0.0,
                        skin.hold_cap_size.y - visible / scale,
                        skin.hold_cap_size.x,
                        skin.hold_cap_size.y,
                    )
                });
                set_rect(&mut sprite, rect);
                sprite.custom_size = Some(Vec2::new(ARROW_SIZE, visible));
                transform.translation.y = bottom + visible / 2.0;
                set_visibility(&mut visibility, Visibility::Visible);
            }
        }
    }
}

/// Mines scroll like arrows and spin as they go.
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

/// Faded arrows freeze in place, fade out, and despawn.
fn fade_graded_arrows(
    time: Res<Time>,
    mut commands: Commands,
    mut arrows: Query<(Entity, &mut ArrowFade, &mut Sprite)>,
) {
    for (entity, mut fade, mut sprite) in &mut arrows {
        fade.remaining -= time.delta_secs();
        if fade.remaining <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let alpha = fade.remaining / fade.total;
        sprite.color = sprite.color.with_alpha(alpha);
    }
}

/// Popups grow slightly and fade away.
#[allow(clippy::type_complexity)]
fn fade_popups(
    time: Res<Time>,
    mut commands: Commands,
    mut popups: Query<(
        Entity,
        &mut Popup,
        &mut Transform,
        Option<&mut TextColor>,
        Option<&mut Sprite>,
    )>,
) {
    for (entity, mut popup, mut transform, text_color, sprite) in &mut popups {
        popup.remaining -= time.delta_secs();
        if popup.remaining <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let alpha = popup.remaining / popup.total;
        transform.scale = Vec3::splat(1.0 + 0.25 * (1.0 - alpha));
        if let Some(mut color) = text_color {
            color.0.set_alpha(alpha);
        }
        if let Some(mut sprite) = sprite {
            sprite.color = sprite.color.with_alpha(alpha);
        }
    }
}

/// Assigns only on change, so unchanged entities don't get flagged for
/// re-extraction every frame.
fn set_atlas_index(sprite: &mut Mut<Sprite>, index: usize) {
    if sprite
        .texture_atlas
        .as_ref()
        .is_some_and(|atlas| atlas.index != index)
        && let Some(atlas) = &mut sprite.texture_atlas
    {
        atlas.index = index;
    }
}

/// Assigns only on change, so unchanged entities don't get flagged for
/// re-extraction every frame.
fn set_rect(sprite: &mut Mut<Sprite>, rect: Option<Rect>) {
    if sprite.rect != rect {
        sprite.rect = rect;
    }
}

/// Assigns only on change, so unchanged entities don't get flagged for
/// re-extraction every frame.
fn set_visibility(visibility: &mut Mut<Visibility>, wanted: Visibility) {
    if **visibility != wanted {
        **visibility = wanted;
    }
}

fn ease_in_out_cubic(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}
