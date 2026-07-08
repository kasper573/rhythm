mod info_panel;
mod player_options;
mod preview;

use crate::core::assets::asset_server_path;
use crate::core::config::GameConfig;
use crate::core::font::game_font;
use crate::core::high_scores::{HighScores, highscore_key};
use crate::core::input::{Actions, GameAction, NavPulse};
use crate::core::library::{StepfileId, StepfileLibrary};
use crate::core::scene_flow::SpawnScoped;
use crate::core::sfx::{PlaySfx, Sfx};
use crate::core::stepfile::{Difficulty, Stepfile, StepsType};
use crate::core::units::Seconds;
use crate::core::{SCREEN_SIZE, ViewportCover, at};
use crate::scenes::{GameScene, SceneFade, scene_accepts_input};
use bevy::ecs::query::QueryData;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::sprite::{Anchor, SpriteImageMode, SpriteScalingMode};

/// The file player scene's entry param.
#[derive(Resource, Debug, Clone, Copy)]
pub struct SelectedStepfile {
    pub id: StepfileId,
    /// Index into the stepfile's `charts`.
    pub chart: usize,
}

/// Which wheel row the file select scene lands on: inserted by whichever
/// scene navigates here wanting a specific row active, consumed on enter.
/// Torn-down scenes keep no state of their own — like route params.
#[derive(Resource, Debug, Clone, Copy)]
pub enum FileSelectTarget {
    Group(usize),
    Stepfile(StepfileId),
}

/// Whether the player is browsing the wheel or editing options in the modal
/// on top of it; input routes to exactly one of the two.
#[derive(SubStates, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[source(GameScene = GameScene::FileSelect)]
pub enum FileSelectMode {
    #[default]
    Browse,
    PlayerOptions,
}

pub struct FileSelectPlugin;

impl Plugin for FileSelectPlugin {
    fn build(&self, app: &mut App) {
        app.add_sub_state::<FileSelectMode>()
            .add_plugins(player_options::plugin)
            .add_message::<WheelTap>()
            .init_resource::<PreferredDifficulty>()
            .add_systems(OnEnter(GameScene::FileSelect), enter)
            .add_systems(OnExit(GameScene::FileSelect), exit)
            .add_systems(OnEnter(FileSelectMode::Browse), clear_nav_pulses)
            .add_systems(OnEnter(FileSelectMode::PlayerOptions), clear_nav_pulses)
            .add_systems(
                Update,
                (
                    (
                        navigate,
                        change_difficulty,
                        track_select,
                        handle_tap,
                        cancel,
                    )
                        .run_if(scene_accepts_input.and_then(in_state(FileSelectMode::Browse))),
                    fit_wheel_rows,
                    animate_wheel,
                    pin_rating_column,
                    preview::pulse_active_row,
                    // The refreshers all observe `Wheel::dirty`; the info
                    // panel runs last and clears it.
                    refresh_scene_background,
                    fade_scene_background,
                    refresh_wheel_rows,
                    refresh_wheel_ratings,
                    info_panel::refresh_info_panel,
                    preview::update_preview,
                )
                    .chain()
                    .run_if(in_state(GameScene::FileSelect).and_then(resource_exists::<Wheel>)),
            );
    }
}

/// Message readers only advance while their mode is active, so switching
/// modes would replay the pulses buffered in between to the other reader.
fn clear_nav_pulses(mut pulses: ResMut<Messages<NavPulse>>) {
    pulses.clear();
}

/// The difficulty rank the player is aiming for, kept across stepfiles and
/// scene visits; each stepfile snaps to its nearest available chart.
#[derive(Resource)]
struct PreferredDifficulty(u8);

impl Default for PreferredDifficulty {
    fn default() -> Self {
        PreferredDifficulty(Difficulty::Medium.rank())
    }
}

const ROW_HEIGHT: f32 = 56.0;
const BAR_WIDTH: f32 = 660.0;
const BAR_HEIGHT: f32 = 50.0;
/// Bar center of the middle row; bars reach past the right screen edge.
const WHEEL_X: f32 = 330.0;
/// Rows shift right as they leave the center, curving the wheel.
const BULGE_PER_ROW: f32 = 3.0;
const BANNER_SIZE: Vec2 = Vec2::new(DETAILS_BOX_SIZE.x, 168.0);
/// On-screen height of the wheel's rating images (native art is 120px).
const RATING_HEIGHT: f32 = 38.0;
/// Right edge of the rating column in row-local coordinates at zero
/// bulge; [`pin_rating_column`] cancels the bulge so the column stays
/// fixed on screen while the bars slide beneath it.
const RATING_RIGHT_X: f32 = 270.0;

const BACKDROP_COLOR: Color = Color::srgb(0.05, 0.085, 0.03);
const STEPFILE_BAR: Color = Color::srgb(0.10, 0.19, 0.07);
const GROUP_BAR: Color = Color::srgb(0.055, 0.10, 0.045);
const BORDER_COLOR: Color = Color::srgb(0.97, 1.0, 0.62);
const STEPFILE_TEXT: Color = Color::srgb(0.35, 0.95, 0.4);
const ACTIVE_STEPFILE_TEXT: Color = Color::srgb(0.8, 1.0, 0.75);
const GROUP_TEXT: Color = Color::srgb(0.95, 0.55, 0.15);
const ARTIST_TEXT: Color = Color::srgb(0.25, 0.75, 0.35);
const BPM_TEXT: Color = Color::srgb(0.85, 0.95, 0.55);
const BANNER_TINT: Color = Color::srgb(0.10, 0.18, 0.07);
const BANNER_TEXT: Color = Color::srgb(0.9, 1.0, 0.85);
const STATS_TEXT: Color = Color::srgb(0.75, 0.9, 0.7);
const HELP_TEXT: Color = Color::srgb(0.5, 0.62, 0.5);

#[derive(Resource)]
struct Wheel {
    entries: Vec<WheelEntry>,
    active: usize,
    /// Spawned row slots; enough to fill the window's visible height,
    /// recomputed by [`fit_wheel_rows`] when that changes.
    slots: usize,
    /// Rows of visual displacement remaining from recent navigation; eased
    /// back to zero every frame so the active item spins into the center.
    scroll_offset: f32,
    expanded_group: Option<usize>,
    /// The generated rounded-gradient texture shared by bars and panels.
    bar_image: Handle<Image>,
    dirty: bool,
}

#[derive(Clone, Copy)]
enum WheelEntry {
    Group { index: usize },
    Stepfile { id: StepfileId },
}

#[derive(Component, Clone, Copy, FromTemplate)]
struct WheelSlot(usize);

#[derive(Component, Default, Clone)]
struct SlotRoot;

/// The frame over the center slot; its opacity pulses with the preview
/// music's beat.
#[derive(Component, Default, Clone)]
struct ActiveRowHighlight;

/// One layer of the scene background wash: the active stepfile's
/// background image over the green backdrop. Changing rows cross-fades:
/// the incoming layer waits invisible until its image has actually
/// loaded, then retires every older layer while it eases in — so the old
/// background always fades under a renderable new one, never against a
/// gap that the late-loading image would pop into.
#[derive(Component, Clone, FromTemplate)]
struct SceneBackground {
    /// The opacity this layer eases toward; reaching zero retires it.
    target: f32,
    /// Spawn order; the newest layer leads and retires the older ones.
    sequence: u32,
}

const BACKGROUND_OPACITY: f32 = 0.25;
/// The contrast box behind the stepfile details column. The banner sits
/// flush against its top and sides; only the content below is padded.
const DETAILS_BOX_SIZE: Vec2 = Vec2::new(540.0, 530.0);
const DETAILS_BOX_CENTER: Vec2 = Vec2::new(-320.0, 12.0);
/// Composites like a 50% black overlay: blending happens on linear color,
/// so matching an sRGB-space half-black needs `1 - 0.5^2.2`.
const DETAILS_BOX_ALPHA: f32 = 0.78;
/// The wheel's exponential settle rate, shared by the background
/// cross-fade so both animations move in lockstep.
const WHEEL_EASE_RATE: f32 = 14.0;

#[derive(Component, Default, Clone)]
struct SlotTitle;

#[derive(Component, Default, Clone)]
struct SlotArtist;

/// The high-score rating image at the right edge of a stepfile row.
#[derive(Component, Default, Clone)]
struct SlotRating;

fn enter(
    mut commands: Commands,
    library: Res<StepfileLibrary>,
    config: Res<GameConfig>,
    target: Option<Res<FileSelectTarget>>,
    windows: Query<&Window>,
    mut images: ResMut<Assets<Image>>,
) {
    // Only the target row's group starts expanded.
    let target = target
        .map(|target| *target)
        .or_else(|| wheel_default_selection(&library, &config).map(FileSelectTarget::Stepfile))
        .or_else(|| {
            (!library.is_empty()).then_some(FileSelectTarget::Stepfile(StepfileId {
                group: 0,
                stepfile: 0,
            }))
        });
    commands.remove_resource::<FileSelectTarget>();
    let expanded_group = target.map(|target| match target {
        FileSelectTarget::Group(index) => index,
        FileSelectTarget::Stepfile(id) => id.group,
    });
    let entries = build_entries(&library, expanded_group);
    let active = target
        .and_then(|target| {
            entries.iter().position(|entry| match (target, entry) {
                (FileSelectTarget::Stepfile(id), WheelEntry::Stepfile { id: entry_id }) => {
                    *entry_id == id
                }
                (FileSelectTarget::Group(index), WheelEntry::Group { index: entry_index }) => {
                    *entry_index == index
                }
                _ => false,
            })
        })
        .unwrap_or(0);
    let bar_image = images.add(rounded_image(512, 64, 16.0, None));

    commands.spawn_scoped(
        GameScene::FileSelect,
        bsn! {
            ViewportCover
            Sprite {
                color: {BACKDROP_COLOR},
                custom_size: {Some(SCREEN_SIZE)},
            }
        },
    );
    let details_box = images.add(rounded_image(
        DETAILS_BOX_SIZE.x as u32,
        DETAILS_BOX_SIZE.y as u32,
        5.0,
        None,
    ));
    commands.spawn_scoped(
        GameScene::FileSelect,
        bsn! {
            Sprite {
                image: {details_box},
                color: {Color::srgba(0.0, 0.0, 0.0, DETAILS_BOX_ALPHA)},
                custom_size: {Some(DETAILS_BOX_SIZE)},
            }
            at(DETAILS_BOX_CENTER.x, DETAILS_BOX_CENTER.y, 4.5)
        },
    );

    let slots = windows.single().map(slots_for).unwrap_or(13);
    for slot in 0..slots {
        spawn_slot(&mut commands, slot, slots, bar_image.clone());
    }

    // The active-row frame: a fixed overlay over the center slot that rows
    // slide beneath; once the wheel rests it reads as the row's border.
    let overlay_size = Vec2::new(BAR_WIDTH + 10.0, BAR_HEIGHT + 10.0);
    let overlay_image = images.add(rounded_image(
        overlay_size.x as u32,
        overlay_size.y as u32,
        18.0,
        Some(5.0),
    ));
    commands.spawn_scoped(
        GameScene::FileSelect,
        bsn! {
            ActiveRowHighlight
            Sprite {
                image: {overlay_image},
                color: {BORDER_COLOR},
                custom_size: {Some(overlay_size)},
            }
            at(WHEEL_X, 0.0, 12.0)
        },
    );

    if library.is_empty() {
        commands.spawn_scoped(
            GameScene::FileSelect,
            bsn! {
                game_font(30.0)
                Text2d("No stepfiles found under assets/stepfiles")
                TextColor(Color::srgb(0.9, 0.4, 0.4))
                at(0.0, 0.0, 20.0)
            },
        );
    }

    commands.spawn_scoped(
        GameScene::FileSelect,
        bsn! {
            game_font(20.0)
            Text2d("up/down: change difficulty\nhold select: change options")
            TextColor({HELP_TEXT})
            at(-320.0, -214.0, 5.0)
        },
    );

    commands.insert_resource(Wheel {
        entries,
        active,
        slots,
        scroll_offset: 0.0,
        expanded_group,
        bar_image,
        dirty: true,
    });
    commands.init_resource::<preview::Preview>();
}

fn exit(mut commands: Commands) {
    commands.remove_resource::<Wheel>();
    commands.remove_resource::<preview::Preview>();
}

fn navigate(
    mut pulses: MessageReader<NavPulse>,
    mut wheel: ResMut<Wheel>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    for pulse in pulses.read() {
        if wheel.entries.is_empty() {
            return;
        }
        let len = wheel.entries.len();
        match pulse.action {
            GameAction::Left => {
                wheel.active = (wheel.active + len - 1) % len;
                wheel.scroll_offset += 1.0;
            }
            GameAction::Right => {
                wheel.active = (wheel.active + 1) % len;
                wheel.scroll_offset -= 1.0;
            }
            _ => continue,
        }
        wheel.dirty = true;
        sfx.write(PlaySfx(Sfx::WheelMove));
    }
}

fn change_difficulty(
    actions: Actions,
    mut wheel: ResMut<Wheel>,
    mut preferred: ResMut<PreferredDifficulty>,
    library: Res<StepfileLibrary>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    let mut delta: i32 = 0;
    if actions.just_pressed(GameAction::Up) {
        delta += 1;
    }
    if actions.just_pressed(GameAction::Down) {
        delta -= 1;
    }
    if delta == 0 {
        return;
    }
    let Some(WheelEntry::Stepfile { id }) = wheel.entries.get(wheel.active) else {
        return;
    };
    let stepfile = &library.stepfile(*id).stepfile;
    let charts = playable_chart_indices(stepfile);
    let Some(current) = chart_for_preference(stepfile, preferred.0) else {
        return;
    };
    let position = charts
        .iter()
        .position(|&index| index == current)
        .expect("current chart comes from the same list");
    let new_position = (position as i32 + delta).clamp(0, charts.len() as i32 - 1) as usize;
    if new_position != position {
        preferred.0 = stepfile.charts[charts[new_position]].difficulty.rank();
        wheel.dirty = true;
        sfx.write(PlaySfx(Sfx::Navigate));
    }
}

/// How long ¤Select¤ must be held to open the player options.
const OPTIONS_HOLD: Seconds = Seconds(0.5);

/// A completed ¤Select¤ tap on the wheel, recognized by [`track_select`]
/// and acted on by [`handle_tap`].
#[derive(Message)]
struct WheelTap;

/// The ¤Select¤ hold state: only presses that began in browse are armed —
/// ¤Select¤ also closes the modal, and that press must not tap when browse
/// resumes.
#[derive(Default)]
struct SelectHold {
    held: Seconds,
    armed: bool,
}

/// Recognizes the ¤Select¤ gesture: holding opens the player options modal,
/// a shorter tap is passed on as a [`WheelTap`].
fn track_select(
    actions: Actions,
    time: Res<Time>,
    mut hold: Local<SelectHold>,
    wheel: Res<Wheel>,
    mut taps: MessageWriter<WheelTap>,
    mut sfx: MessageWriter<PlaySfx>,
    mut mode: ResMut<NextState<FileSelectMode>>,
) {
    if wheel.entries.is_empty() {
        return;
    }
    if actions.just_pressed(GameAction::Select) {
        hold.armed = true;
        hold.held = Seconds::ZERO;
    }
    if !hold.armed {
        return;
    }
    if actions.pressed(GameAction::Select) {
        hold.held += Seconds(time.delta_secs_f64());
        if hold.held >= OPTIONS_HOLD {
            hold.armed = false;
            sfx.write(PlaySfx(Sfx::Select));
            mode.set(FileSelectMode::PlayerOptions);
        }
        return;
    }
    if actions.just_released(GameAction::Select) {
        hold.armed = false;
        taps.write(WheelTap);
    }
}

/// A tap acts on the active row: groups toggle open, stepfiles start.
fn handle_tap(
    mut taps: MessageReader<WheelTap>,
    mut wheel: ResMut<Wheel>,
    library: Res<StepfileLibrary>,
    preferred: Res<PreferredDifficulty>,
    mut commands: Commands,
    mut sfx: MessageWriter<PlaySfx>,
    mut fade: ResMut<SceneFade>,
) {
    for _ in taps.read() {
        sfx.write(PlaySfx(Sfx::WheelSelect));
        match wheel.entries[wheel.active] {
            WheelEntry::Group { index } => {
                // Only one group is ever expanded: opening a group closes
                // the previous one, opening it again closes it.
                wheel.expanded_group = (wheel.expanded_group != Some(index)).then_some(index);
                wheel.entries = build_entries(&library, wheel.expanded_group);
                wheel.active = wheel
                    .entries
                    .iter()
                    .position(
                        |entry| matches!(entry, WheelEntry::Group { index: i } if *i == index),
                    )
                    .unwrap_or(0);
                wheel.dirty = true;
                sfx.write(PlaySfx(Sfx::GroupToggle));
            }
            WheelEntry::Stepfile { id } => {
                let stepfile = &library.stepfile(id).stepfile;
                let chart = chart_for_preference(stepfile, preferred.0).unwrap_or(0);
                commands.insert_resource(SelectedStepfile { id, chart });
                sfx.write(PlaySfx(Sfx::StartFile));
                fade.begin(GameScene::FilePlayer);
            }
        }
    }
}

fn cancel(actions: Actions, mut fade: ResMut<SceneFade>, mut sfx: MessageWriter<PlaySfx>) {
    if actions.just_pressed(GameAction::Cancel) {
        sfx.write(PlaySfx(Sfx::Cancel));
        fade.begin(GameScene::MainMenu);
    }
}

fn animate_wheel(
    time: Res<Time>,
    mut wheel: ResMut<Wheel>,
    mut roots: Query<(&WheelSlot, &mut Transform), With<SlotRoot>>,
) {
    if wheel.scroll_offset != 0.0 {
        wheel.scroll_offset *= (-WHEEL_EASE_RATE * time.delta_secs()).exp();
        if wheel.scroll_offset.abs() < 0.01 {
            wheel.scroll_offset = 0.0;
        }
    }
    for (slot, mut transform) in &mut roots {
        transform.translation.x = slot_x(slot.0, wheel.slots, wheel.scroll_offset);
        transform.translation.y = slot_y(slot.0, wheel.slots, wheel.scroll_offset);
    }
}

fn pin_rating_column(
    wheel: Res<Wheel>,
    mut ratings: Query<(&WheelSlot, &mut Transform), With<SlotRating>>,
) {
    for (slot, mut transform) in &mut ratings {
        let bulge = slot_x(slot.0, wheel.slots, wheel.scroll_offset) - WHEEL_X;
        transform.translation.x = RATING_RIGHT_X - bulge;
    }
}

#[derive(QueryData)]
#[query_data(mutable)]
struct SlotTitleText {
    slot: &'static WheelSlot,
    text: &'static mut Text2d,
    color: &'static mut TextColor,
    transform: &'static mut Transform,
}

fn refresh_wheel_rows(
    wheel: Res<Wheel>,
    library: Res<StepfileLibrary>,
    mut roots: Query<(&WheelSlot, &mut Sprite), With<SlotRoot>>,
    mut titles: Query<SlotTitleText, (With<SlotTitle>, Without<SlotArtist>)>,
    mut artists: Query<(&WheelSlot, &mut Text2d, &mut TextColor), With<SlotArtist>>,
) {
    if !wheel.dirty {
        return;
    }
    for (slot, mut sprite) in &mut roots {
        sprite.color = match slot_entry(&wheel, slot.0) {
            Some(WheelEntry::Group { .. }) => GROUP_BAR,
            _ => STEPFILE_BAR,
        };
    }
    for mut title in &mut titles {
        let is_center = title.slot.0 == wheel.slots / 2;
        match slot_entry(&wheel, title.slot.0) {
            Some(WheelEntry::Group { index }) => {
                let group = &library.groups[*index];
                title.text.0 = group.name.clone();
                title.color.0 = GROUP_TEXT;
                title.transform.translation.y = 0.0;
            }
            Some(WheelEntry::Stepfile { id }) => {
                let entry = library.stepfile(*id);
                title.text.0 = entry.display_title();
                title.color.0 = if is_center {
                    ACTIVE_STEPFILE_TEXT
                } else {
                    STEPFILE_TEXT
                };
                title.transform.translation.y = if entry.display_artist().is_empty() {
                    0.0
                } else {
                    9.0
                };
            }
            None => title.text.0 = String::new(),
        }
    }
    for (slot, mut text, mut color) in &mut artists {
        match slot_entry(&wheel, slot.0) {
            Some(WheelEntry::Stepfile { id }) => {
                let artist = library.stepfile(*id).display_artist();
                text.0 = match artist.is_empty() {
                    true => String::new(),
                    false => format!("/ {artist}"),
                };
                color.0 = ARTIST_TEXT;
            }
            _ => text.0 = String::new(),
        }
    }
}

fn refresh_scene_background(
    wheel: Res<Wheel>,
    library: Res<StepfileLibrary>,
    asset_server: Res<AssetServer>,
    mut layers: Query<(&mut SceneBackground, &Sprite)>,
    mut commands: Commands,
    mut layer_count: Local<u32>,
) {
    if !wheel.dirty {
        return;
    }
    let path = match wheel.entries.get(wheel.active) {
        Some(WheelEntry::Stepfile { id }) => library
            .stepfile(*id)
            .background_path()
            .as_deref()
            .and_then(asset_server_path),
        _ => None,
    };
    let desired = path.map(|path| asset_server.load::<Image>(path));
    let Some(handle) = desired else {
        // Nothing incoming (a group row): fade everything out.
        for (mut layer, _) in &mut layers {
            layer.target = 0.0;
        }
        return;
    };
    let already_shown = layers
        .iter()
        .any(|(layer, sprite)| layer.target > 0.0 && sprite.image == handle);
    if already_shown {
        return;
    }
    // Newer layers draw above the ones fading out; the small cycling bump
    // stays well below everything else in the scene.
    *layer_count += 1;
    let z = 0.5 + (*layer_count % 100) as f32 * 0.002;
    let sequence = *layer_count;
    commands.spawn_scoped(
        GameScene::FileSelect,
        bsn! {
            SceneBackground { target: {BACKGROUND_OPACITY}, sequence: {sequence} }
            ViewportCover
            Sprite {
                image: {handle},
                color: Color::srgba(1.0, 1.0, 1.0, 0.0),
                custom_size: {Some(SCREEN_SIZE)},
                image_mode: {SpriteImageMode::Scale(SpriteScalingMode::FillCenter)},
            }
            at(0.0, 0.0, z)
        },
    );
}

/// Eases every background layer toward its target opacity at the wheel's
/// settle rate and retires the fully faded-out ones. Layers whose image is
/// still loading hold at zero: only a loaded layer may lead, and only the
/// leader retires the layers beneath it.
fn fade_scene_background(
    time: Res<Time>,
    images: Res<Assets<Image>>,
    mut layers: Query<(Entity, &mut SceneBackground, &mut Sprite)>,
    mut commands: Commands,
) {
    let leader = layers
        .iter()
        .filter(|(_, layer, sprite)| layer.target > 0.0 && images.contains(&sprite.image))
        .max_by_key(|(_, layer, _)| layer.sequence)
        .map(|(entity, layer, _)| (entity, layer.sequence));
    if let Some((leader, leader_sequence)) = leader {
        for (entity, mut layer, _) in &mut layers {
            if entity != leader && layer.sequence < leader_sequence && layer.target > 0.0 {
                layer.target = 0.0;
            }
        }
    }

    let ease = 1.0 - (-WHEEL_EASE_RATE * time.delta_secs()).exp();
    for (entity, layer, mut sprite) in &mut layers {
        if layer.target > 0.0 && !images.contains(&sprite.image) {
            continue;
        }
        let alpha = sprite.color.alpha();
        let mut next = alpha + (layer.target - alpha) * ease;
        if (next - layer.target).abs() < 0.002 {
            next = layer.target;
        }
        if next != alpha {
            sprite.color.set_alpha(next);
        }
        if layer.target <= 0.0 && next <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

fn refresh_wheel_ratings(
    wheel: Res<Wheel>,
    library: Res<StepfileLibrary>,
    high_scores: Res<HighScores>,
    config: Res<GameConfig>,
    preferred: Res<PreferredDifficulty>,
    asset_server: Res<AssetServer>,
    mut ratings: Query<(&WheelSlot, &mut Sprite, &mut Visibility), With<SlotRating>>,
) {
    if !wheel.dirty {
        return;
    }
    for (slot, mut sprite, mut visibility) in &mut ratings {
        let shown = match slot_entry(&wheel, slot.0) {
            Some(WheelEntry::Stepfile { id }) => {
                high_score_rating(*id, &library, &high_scores, &config, preferred.0).map(|image| {
                    let image = asset_server.load(image);
                    if sprite.image != image {
                        sprite.image = image;
                    }
                })
            }
            _ => None,
        };
        visibility.set_if_neq(match shown {
            Some(()) => Visibility::Visible,
            None => Visibility::Hidden,
        });
    }
}

/// The rating image earned by the high score of the chart this row would
/// currently play. Only the total points are stored, so grade-based
/// rating rules never match here.
fn high_score_rating(
    id: StepfileId,
    library: &StepfileLibrary,
    high_scores: &HighScores,
    config: &GameConfig,
    preferred: u8,
) -> Option<String> {
    let entry = library.stepfile(id);
    let chart = &entry.stepfile.charts[chart_for_preference(&entry.stepfile, preferred)?];
    let key = highscore_key(library.group_name(id), &entry.name(), &chart.difficulty);
    let points = high_scores.get(&key)?;
    let stats = chart.stats();
    let percent = config.score_percent(points, chart.rows.len() as u32, stats.holds as u32);
    Some(config.rating(percent, None).image.clone())
}

fn slot_y(slot: usize, slots: usize, scroll_offset: f32) -> f32 {
    ((slots / 2) as f32 - slot as f32 + scroll_offset) * ROW_HEIGHT
}

/// Rows curve away to the right as they leave the center, like the visible
/// edge of a wheel.
fn slot_x(slot: usize, slots: usize, scroll_offset: f32) -> f32 {
    let rows_from_center = (slots / 2) as f32 - slot as f32 + scroll_offset;
    WHEEL_X + BULGE_PER_ROW * rows_from_center * rows_from_center
}

fn slot_entry(wheel: &Wheel, slot: usize) -> Option<&WheelEntry> {
    if wheel.entries.is_empty() {
        return None;
    }
    let len = wheel.entries.len() as i64;
    let index = (wheel.active as i64 + slot as i64 - (wheel.slots / 2) as i64).rem_euclid(len);
    wheel.entries.get(index as usize)
}

/// Slots needed to fill the window's visible world height — the camera
/// shows more than the canvas when the window is taller than 16:9 — plus
/// one above and below so scrolling never reveals a gap, forced odd so a
/// center slot exists.
fn slots_for(window: &Window) -> usize {
    let width = window.width().max(1.0);
    let height = window.height().max(1.0);
    let visible_height = height * (SCREEN_SIZE.x / width).max(SCREEN_SIZE.y / height);
    ((visible_height / ROW_HEIGHT).ceil() as usize + 2) | 1
}

/// Respawns the wheel rows when the window's visible height changes how
/// many are needed.
fn fit_wheel_rows(
    windows: Query<&Window, Changed<Window>>,
    mut wheel: ResMut<Wheel>,
    slot_roots: Query<Entity, With<SlotRoot>>,
    mut commands: Commands,
) {
    let Ok(window) = windows.single() else { return };
    let slots = slots_for(window);
    if slots == wheel.slots {
        return;
    }
    wheel.slots = slots;
    wheel.dirty = true;
    for entity in &slot_roots {
        commands.entity(entity).despawn();
    }
    for slot in 0..slots {
        spawn_slot(&mut commands, slot, slots, wheel.bar_image.clone());
    }
}

fn spawn_slot(commands: &mut Commands, slot: usize, slots: usize, bar: Handle<Image>) {
    commands.spawn_scoped(
        GameScene::FileSelect,
        bsn! {
            WheelSlot(slot)
            SlotRoot
            Sprite {
                image: {bar},
                color: {STEPFILE_BAR},
                custom_size: {Some(Vec2::new(BAR_WIDTH, BAR_HEIGHT))},
            }
            at(slot_x(slot, slots, 0.0), slot_y(slot, slots, 0.0), 10.0)
            Children [
                (
                    WheelSlot(slot)
                    SlotTitle
                    game_font(26.0)
                    Text2d("")
                    TextColor({STEPFILE_TEXT})
                    Anchor({Anchor::CENTER_LEFT.0})
                    at(-BAR_WIDTH / 2.0 + 26.0, 9.0, 0.1)
                ),
                (
                    WheelSlot(slot)
                    SlotArtist
                    game_font(17.0)
                    Text2d("")
                    TextColor({ARTIST_TEXT})
                    Anchor({Anchor::CENTER_LEFT.0})
                    at(-BAR_WIDTH / 2.0 + 60.0, -15.0, 0.1)
                ),
                (
                    WheelSlot(slot)
                    SlotRating
                    Sprite
                    Anchor({Anchor::CENTER_RIGHT.0})
                    Visibility::Hidden
                    Transform {
                        translation: {Vec3::new(RATING_RIGHT_X, 0.0, 0.1)},
                        scale: {Vec3::splat(RATING_HEIGHT / 120.0)},
                    }
                ),
            ]
        },
    );
}

fn build_entries(library: &StepfileLibrary, expanded_group: Option<usize>) -> Vec<WheelEntry> {
    let mut entries = Vec::new();
    for (group_index, group) in library.groups.iter().enumerate() {
        let is_expanded = expanded_group == Some(group_index);
        entries.push(WheelEntry::Group { index: group_index });
        if !is_expanded {
            continue;
        }
        for stepfile_index in 0..group.stepfiles.len() {
            entries.push(WheelEntry::Stepfile {
                id: StepfileId {
                    group: group_index,
                    stepfile: stepfile_index,
                },
            });
        }
    }
    entries
}

/// Resolves the configured `wheel_default` `(group, stepfile)` search pair:
/// the first group whose name contains the group string and that holds a
/// stepfile whose title contains the stepfile string, both case-insensitive.
fn wheel_default_selection(library: &StepfileLibrary, config: &GameConfig) -> Option<StepfileId> {
    let (group_search, stepfile_search) = &config.wheel_default;
    let group_search = group_search.to_lowercase();
    let stepfile_search = stepfile_search.to_lowercase();
    for (group_index, group) in library.groups.iter().enumerate() {
        if !group.name.to_lowercase().contains(&group_search) {
            continue;
        }
        let stepfile_index = group.stepfiles.iter().position(|entry| {
            entry
                .display_title()
                .to_lowercase()
                .contains(&stepfile_search)
        });
        if let Some(stepfile_index) = stepfile_index {
            return Some(StepfileId {
                group: group_index,
                stepfile: stepfile_index,
            });
        }
    }
    None
}

/// Indices of the playable (dance-single, non-empty) charts, easiest first.
fn playable_chart_indices(stepfile: &Stepfile) -> Vec<usize> {
    let mut charts: Vec<usize> = stepfile
        .charts
        .iter()
        .enumerate()
        .filter(|(_, chart)| chart.steps_type == StepsType::DanceSingle && !chart.rows.is_empty())
        .map(|(index, _)| index)
        .collect();
    charts.sort_by_key(|&index| {
        let chart = &stepfile.charts[index];
        (chart.difficulty.rank(), chart.meter)
    });
    charts
}

/// The chart whose difficulty is closest to the preferred rank.
fn chart_for_preference(stepfile: &Stepfile, preferred: u8) -> Option<usize> {
    playable_chart_indices(stepfile)
        .into_iter()
        .min_by_key(|&index| {
            let rank = stepfile.charts[index].difficulty.rank();
            ((rank as i16 - preferred as i16).abs(), rank)
        })
}

/// A white vertical-gradient rounded rectangle for sprites to tint: every
/// bar and panel in this scene, and — with a `hollow_border` — the
/// active-row frame, whose interior fades to a faint wash so the rows
/// beneath stay readable. Generated at the exact size it is drawn so edges
/// and ring stay uniformly thick.
fn rounded_image(width: u32, height: u32, radius: f32, hollow_border: Option<f32>) -> Image {
    const INTERIOR_WASH: f32 = 0.18;
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let brightness = 255.0 - 130.0 * (y as f32 / (height - 1) as f32);
        for x in 0..width {
            let to_edge_x =
                (x as f32 + 0.5 - width as f32 / 2.0).abs() - (width as f32 / 2.0 - radius);
            let to_edge_y =
                (y as f32 + 0.5 - height as f32 / 2.0).abs() - (height as f32 / 2.0 - radius);
            let distance = Vec2::new(to_edge_x.max(0.0), to_edge_y.max(0.0)).length() - radius;
            let mut alpha = (0.5 - distance).clamp(0.0, 1.0);
            if let Some(border) = hollow_border {
                let interior = (-distance - border).clamp(0.0, 1.0);
                alpha *= 1.0 - interior * (1.0 - INTERIOR_WASH);
            }
            data.extend_from_slice(&[
                brightness as u8,
                brightness as u8,
                brightness as u8,
                (alpha * 255.0) as u8,
            ]);
        }
    }
    Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        Default::default(),
    )
}
