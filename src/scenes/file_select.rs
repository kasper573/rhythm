use crate::core::SCREEN_SIZE;
use crate::core::assets::asset_server_path;
use crate::core::config::GameConfig;
use crate::core::font::GameFont;
use crate::core::input::{Actions, GameAction, NavPulse};
use crate::core::library::{StepfileId, StepfileLibrary};
use crate::core::scene_flow::{GameScene, SceneFade, scene_accepts_input};
use crate::core::sfx::{PlaySfx, Sfx};
use crate::core::stepfile::{Difficulty, DisplayBpm, Stepfile, StepsType};
use crate::core::units::Seconds;
use bevy::audio::PlaybackMode;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::sprite::Anchor;
use std::time::Duration;

/// The stepfile and chart the player picked; the file player scene requires it.
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

pub struct FileSelectPlugin;

impl Plugin for FileSelectPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PreferredDifficulty>()
            .add_systems(OnEnter(GameScene::FileSelect), enter)
            .add_systems(OnExit(GameScene::FileSelect), exit)
            .add_systems(
                Update,
                (
                    (navigate, change_difficulty, select, cancel).run_if(scene_accepts_input),
                    animate_wheel,
                    // Both refreshers observe `Wheel::dirty`; the info panel
                    // runs last and clears it.
                    refresh_wheel_rows,
                    refresh_info_panel,
                    update_preview,
                )
                    .chain()
                    .run_if(in_state(GameScene::FileSelect).and_then(resource_exists::<Wheel>)),
            );
    }
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

const SLOTS: usize = 13;
const CENTER: usize = SLOTS / 2;
const ROW_HEIGHT: f32 = 56.0;
const BAR_WIDTH: f32 = 660.0;
const BAR_HEIGHT: f32 = 50.0;
/// Bar center of the middle row; bars reach past the right screen edge.
const WHEEL_X: f32 = 330.0;
/// Rows shift right as they leave the center, curving the wheel.
const BULGE_PER_ROW: f32 = 3.0;
const PREVIEW_DEBOUNCE: Seconds = Seconds(0.35);

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

#[derive(Resource)]
struct Wheel {
    entries: Vec<WheelEntry>,
    active: usize,
    /// Rows of visual displacement remaining from recent navigation; eased
    /// back to zero every frame so the active item spins into the center.
    scroll_offset: f32,
    /// The one group whose stepfiles are listed, if any.
    expanded: Option<usize>,
    /// Stepfile whose music the preview aims at, plus its debounce clock.
    preview_stepfile: Option<StepfileId>,
    preview_wait: Seconds,
    preview_entity: Option<Entity>,
    /// The generated rounded-gradient texture shared by bars and panels.
    bar_image: Handle<Image>,
    dirty: bool,
}

#[derive(Clone)]
enum WheelEntry {
    Group { index: usize, label: String },
    Stepfile { id: StepfileId },
}

#[derive(Component, Clone, Copy)]
struct WheelSlot(usize);

#[derive(Component)]
struct SlotRoot;

#[derive(Component)]
struct SlotTitle;

#[derive(Component)]
struct SlotArtist;

#[derive(Component)]
struct InfoBanner;

#[derive(Component)]
struct InfoBannerText;

#[derive(Component)]
struct InfoBpmText;

#[derive(Component)]
struct InfoDifficultyText;

#[derive(Component)]
struct InfoStatsText;

fn enter(
    mut commands: Commands,
    library: Res<StepfileLibrary>,
    config: Res<GameConfig>,
    font: Res<GameFont>,
    target: Option<Res<FileSelectTarget>>,
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
    let expanded = target.map(|target| match target {
        FileSelectTarget::Group(index) => index,
        FileSelectTarget::Stepfile(id) => id.group,
    });
    let entries = build_entries(&library, expanded);
    let active = target
        .and_then(|target| {
            entries.iter().position(|entry| match (target, entry) {
                (FileSelectTarget::Stepfile(id), WheelEntry::Stepfile { id: entry_id }) => {
                    *entry_id == id
                }
                (
                    FileSelectTarget::Group(index),
                    WheelEntry::Group {
                        index: entry_index, ..
                    },
                ) => *entry_index == index,
                _ => false,
            })
        })
        .unwrap_or(0);
    let bar_image = images.add(rounded_image(512, 64, 16.0, None));

    commands.spawn((
        DespawnOnExit(GameScene::FileSelect),
        Sprite {
            color: BACKDROP_COLOR,
            custom_size: Some(SCREEN_SIZE),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));

    for slot in 0..SLOTS {
        commands
            .spawn((
                DespawnOnExit(GameScene::FileSelect),
                WheelSlot(slot),
                SlotRoot,
                Sprite {
                    image: bar_image.clone(),
                    color: STEPFILE_BAR,
                    custom_size: Some(Vec2::new(BAR_WIDTH, BAR_HEIGHT)),
                    ..default()
                },
                Transform::from_xyz(slot_x(slot, 0.0), slot_y(slot, 0.0), 10.0),
            ))
            .with_children(|slot_parent| {
                slot_parent.spawn((
                    WheelSlot(slot),
                    SlotTitle,
                    Text2d::new(""),
                    font.sized(26.0),
                    TextColor(STEPFILE_TEXT),
                    Anchor::CENTER_LEFT,
                    Transform::from_xyz(-BAR_WIDTH / 2.0 + 26.0, 9.0, 0.1),
                ));
                slot_parent.spawn((
                    WheelSlot(slot),
                    SlotArtist,
                    Text2d::new(""),
                    font.sized(17.0),
                    TextColor(ARTIST_TEXT),
                    Anchor::CENTER_LEFT,
                    Transform::from_xyz(-BAR_WIDTH / 2.0 + 60.0, -15.0, 0.1),
                ));
            });
    }

    // The active-row frame: a fixed overlay over the center slot that rows
    // slide beneath; once the wheel rests it reads as the row's border.
    let overlay_size = Vec2::new(BAR_WIDTH + 10.0, BAR_HEIGHT + 10.0);
    commands.spawn((
        DespawnOnExit(GameScene::FileSelect),
        Sprite {
            image: images.add(rounded_image(
                overlay_size.x as u32,
                overlay_size.y as u32,
                18.0,
                Some(5.0),
            )),
            color: BORDER_COLOR,
            custom_size: Some(overlay_size),
            ..default()
        },
        Transform::from_xyz(WHEEL_X, 0.0, 12.0),
    ));

    spawn_info_panel(&mut commands, &font, &bar_image);

    if library.is_empty() {
        commands.spawn((
            DespawnOnExit(GameScene::FileSelect),
            Text2d::new("No stepfiles found under assets/stepfiles"),
            font.sized(30.0),
            TextColor(Color::srgb(0.9, 0.4, 0.4)),
            Transform::from_xyz(0.0, 0.0, 20.0),
        ));
    }

    commands.insert_resource(Wheel {
        entries,
        active,
        scroll_offset: 0.0,
        expanded,
        preview_stepfile: None,
        preview_wait: Seconds::ZERO,
        preview_entity: None,
        bar_image,
        dirty: true,
    });
}

fn exit(mut commands: Commands) {
    commands.remove_resource::<Wheel>();
}

fn spawn_info_panel(commands: &mut Commands, font: &GameFont, bar_image: &Handle<Image>) {
    commands.spawn((
        DespawnOnExit(GameScene::FileSelect),
        InfoBanner,
        Sprite {
            image: bar_image.clone(),
            color: BANNER_TINT,
            custom_size: Some(Vec2::new(500.0, 156.0)),
            ..default()
        },
        Transform::from_xyz(-320.0, 190.0, 5.0),
    ));
    commands.spawn((
        DespawnOnExit(GameScene::FileSelect),
        InfoBannerText,
        Text2d::new(""),
        font.sized(24.0),
        TextColor(BANNER_TEXT),
        Transform::from_xyz(-320.0, 190.0, 5.5),
    ));
    commands.spawn((
        DespawnOnExit(GameScene::FileSelect),
        InfoBpmText,
        Text2d::new(""),
        font.sized(28.0),
        TextColor(BPM_TEXT),
        Transform::from_xyz(-320.0, 70.0, 5.0),
    ));
    commands.spawn((
        DespawnOnExit(GameScene::FileSelect),
        InfoDifficultyText,
        Text2d::new(""),
        font.sized(34.0),
        TextColor(Color::WHITE),
        Transform::from_xyz(-320.0, 18.0, 5.0),
    ));
    commands.spawn((
        DespawnOnExit(GameScene::FileSelect),
        InfoStatsText,
        Text2d::new(""),
        font.sized(22.0),
        TextColor(Color::srgb(0.75, 0.9, 0.7)),
        Transform::from_xyz(-320.0, -70.0, 5.0),
    ));
}

/// ¤Left¤/¤Right¤ spin the wheel.
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
                wheel.scroll_offset -= 1.0;
            }
            GameAction::Right => {
                wheel.active = (wheel.active + 1) % len;
                wheel.scroll_offset += 1.0;
            }
            _ => continue,
        }
        wheel.dirty = true;
        sfx.write(PlaySfx(Sfx::WheelMove));
    }
}

/// ¤Up¤/¤Down¤ pick a harder/easier chart of the active stepfile.
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

/// Tapping ¤Select¤ acts on the active row; holding it opens the player
/// options instead, passing the active row along so coming back lands here.
#[allow(clippy::too_many_arguments)]
fn select(
    actions: Actions,
    time: Res<Time>,
    mut held: Local<Seconds>,
    mut wheel: ResMut<Wheel>,
    library: Res<StepfileLibrary>,
    preferred: Res<PreferredDifficulty>,
    mut commands: Commands,
    mut sfx: MessageWriter<PlaySfx>,
    mut fade: ResMut<SceneFade>,
) {
    if wheel.entries.is_empty() {
        return;
    }
    if actions.pressed(GameAction::Select) {
        let before = *held;
        *held += Seconds(time.delta_secs_f64());
        if before < OPTIONS_HOLD && *held >= OPTIONS_HOLD {
            let row = match &wheel.entries[wheel.active] {
                WheelEntry::Group { index, .. } => FileSelectTarget::Group(*index),
                WheelEntry::Stepfile { id } => FileSelectTarget::Stepfile(*id),
            };
            commands.insert_resource(row);
            sfx.write(PlaySfx(Sfx::Select));
            fade.begin(GameScene::PlayerOptions);
        }
        return;
    }
    let tapped = actions.just_released(GameAction::Select) && *held < OPTIONS_HOLD;
    *held = Seconds::ZERO;
    if !tapped {
        return;
    }

    sfx.write(PlaySfx(Sfx::WheelSelect));
    match wheel.entries[wheel.active].clone() {
        WheelEntry::Group { index, .. } => {
            // Only one group is ever expanded: opening a group closes the
            // previous one, opening it again closes it.
            wheel.expanded = (wheel.expanded != Some(index)).then_some(index);
            wheel.entries = build_entries(&library, wheel.expanded);
            wheel.active = wheel
                .entries
                .iter()
                .position(
                    |entry| matches!(entry, WheelEntry::Group { index: i, .. } if *i == index),
                )
                .unwrap_or(0);
            wheel.dirty = true;
            sfx.write(PlaySfx(Sfx::GroupToggle));
        }
        WheelEntry::Stepfile { id } => {
            if let Some(preview) = wheel.preview_entity.take() {
                commands.entity(preview).try_despawn();
            }
            let stepfile = &library.stepfile(id).stepfile;
            let chart = chart_for_preference(stepfile, preferred.0).unwrap_or(0);
            commands.insert_resource(SelectedStepfile { id, chart });
            sfx.write(PlaySfx(Sfx::StartFile));
            fade.begin(GameScene::FilePlayer);
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
        wheel.scroll_offset *= (-14.0 * time.delta_secs()).exp();
        if wheel.scroll_offset.abs() < 0.01 {
            wheel.scroll_offset = 0.0;
        }
    }
    for (slot, mut transform) in &mut roots {
        transform.translation.x = slot_x(slot.0, wheel.scroll_offset);
        transform.translation.y = slot_y(slot.0, wheel.scroll_offset);
    }
}

#[allow(clippy::type_complexity)]
fn refresh_wheel_rows(
    wheel: Res<Wheel>,
    library: Res<StepfileLibrary>,
    mut roots: Query<(&WheelSlot, &mut Sprite), With<SlotRoot>>,
    mut titles: Query<
        (&WheelSlot, &mut Text2d, &mut TextColor, &mut Transform),
        (With<SlotTitle>, Without<SlotArtist>),
    >,
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
    for (slot, mut text, mut color, mut transform) in &mut titles {
        let is_center = slot.0 == CENTER;
        match slot_entry(&wheel, slot.0) {
            Some(WheelEntry::Group { label, .. }) => {
                text.0 = label.clone();
                color.0 = GROUP_TEXT;
                transform.translation.y = 0.0;
            }
            Some(WheelEntry::Stepfile { id }) => {
                let entry = library.stepfile(*id);
                text.0 = entry.display_title();
                color.0 = if is_center {
                    ACTIVE_STEPFILE_TEXT
                } else {
                    STEPFILE_TEXT
                };
                transform.translation.y = if entry.display_artist().is_empty() {
                    0.0
                } else {
                    9.0
                };
            }
            None => text.0 = String::new(),
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

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn refresh_info_panel(
    mut wheel: ResMut<Wheel>,
    library: Res<StepfileLibrary>,
    preferred: Res<PreferredDifficulty>,
    asset_server: Res<AssetServer>,
    mut banner: Single<&mut Sprite, With<InfoBanner>>,
    mut banner_text: Single<&mut Text2d, (With<InfoBannerText>, Without<InfoBpmText>)>,
    mut bpm_text: Single<&mut Text2d, (With<InfoBpmText>, Without<InfoDifficultyText>)>,
    mut difficulty_text: Single<
        (&mut Text2d, &mut TextColor),
        (With<InfoDifficultyText>, Without<InfoBannerText>),
    >,
    mut stats_text: Single<
        &mut Text2d,
        (
            With<InfoStatsText>,
            Without<InfoBannerText>,
            Without<InfoBpmText>,
            Without<InfoDifficultyText>,
        ),
    >,
) {
    if !wheel.dirty {
        return;
    }
    wheel.dirty = false;

    let (difficulty_label, difficulty_color) = &mut *difficulty_text;
    match wheel.entries.get(wheel.active) {
        Some(WheelEntry::Stepfile { id }) => {
            let entry = library.stepfile(*id);
            match entry.banner_path().as_deref().and_then(asset_server_path) {
                Some(path) => {
                    banner.image = asset_server.load(path);
                    banner.color = Color::WHITE;
                    banner_text.0 = String::new();
                }
                None => {
                    banner.image = wheel.bar_image.clone();
                    banner.color = BANNER_TINT;
                    banner_text.0 = entry.display_title();
                }
            }
            bpm_text.0 = bpm_label(&entry.stepfile);
            match chart_for_preference(&entry.stepfile, preferred.0) {
                Some(index) => {
                    let chart = &entry.stepfile.charts[index];
                    let (name, color) = difficulty_style(&chart.difficulty);
                    difficulty_label.0 = format!("{name} {}", chart.meter);
                    difficulty_color.0 = color;
                    stats_text.0 = stats_label(&entry.stepfile, index);
                }
                None => {
                    difficulty_label.0 = String::new();
                    stats_text.0 = String::new();
                }
            }
        }
        Some(WheelEntry::Group { index, .. }) => {
            let group = &library.groups[*index];
            match group.banner_path.as_deref().and_then(asset_server_path) {
                Some(path) => {
                    banner.image = asset_server.load(path);
                    banner.color = Color::WHITE;
                    banner_text.0 = String::new();
                }
                None => {
                    banner.image = wheel.bar_image.clone();
                    banner.color = BANNER_TINT;
                    banner_text.0 = group.name.clone();
                }
            }
            bpm_text.0 = match group.stepfiles.len() {
                1 => "1 stepfile".to_string(),
                count => format!("{count} stepfiles"),
            };
            difficulty_label.0 = String::new();
            stats_text.0 = String::new();
        }
        None => {
            banner_text.0 = String::new();
            bpm_text.0 = String::new();
            difficulty_label.0 = String::new();
            stats_text.0 = String::new();
        }
    }
}

/// Chart facts for the info panel: counts and playable length.
fn stats_label(stepfile: &Stepfile, chart_index: usize) -> String {
    let chart = &stepfile.charts[chart_index];
    let stats = chart.stats();
    let duration = chart
        .last_note_beat()
        .map(|beat| stepfile.timing.seconds_at_beat(beat))
        .unwrap_or(Seconds::ZERO);
    let minutes = (duration.0.max(0.0) / 60.0) as u32;
    let seconds = (duration.0.max(0.0) % 60.0) as u32;
    format!(
        "Steps {}   Jumps {}\nHolds {}   Mines {}\nLength {minutes}:{seconds:02}",
        stats.steps, stats.jumps, stats.holds, stats.mines
    )
}

fn update_preview(
    time: Res<Time>,
    mut wheel: ResMut<Wheel>,
    library: Res<StepfileLibrary>,
    asset_server: Res<AssetServer>,
    mut commands: Commands,
) {
    let active_stepfile = match wheel.entries.get(wheel.active) {
        Some(WheelEntry::Stepfile { id }) => Some(*id),
        _ => None,
    };

    if wheel.preview_stepfile != active_stepfile {
        wheel.preview_stepfile = active_stepfile;
        wheel.preview_wait = Seconds::ZERO;
        if let Some(preview) = wheel.preview_entity.take() {
            commands.entity(preview).try_despawn();
        }
        return;
    }

    let Some(id) = active_stepfile else { return };
    if wheel.preview_entity.is_some() {
        return;
    }
    wheel.preview_wait += Seconds(time.delta_secs_f64());
    if wheel.preview_wait < PREVIEW_DEBOUNCE {
        return;
    }

    let entry = library.stepfile(id);
    let Some(path) = entry.music_path().as_deref().and_then(asset_server_path) else {
        return;
    };
    let stepfile = &entry.stepfile;
    let start = stepfile.sample_start.0.max(0.0);
    let length = stepfile.sample_length.0;
    let entity = commands
        .spawn((
            DespawnOnExit(GameScene::FileSelect),
            AudioPlayer::new(asset_server.load(path)),
            PlaybackSettings {
                mode: PlaybackMode::Loop,
                start_position: Some(Duration::from_secs_f64(start)),
                duration: (length > 0.0).then(|| Duration::from_secs_f64(length)),
                ..default()
            },
        ))
        .id();
    wheel.preview_entity = Some(entity);
}

fn slot_y(slot: usize, scroll_offset: f32) -> f32 {
    (CENTER as f32 - slot as f32 + scroll_offset) * ROW_HEIGHT
}

/// Rows curve away to the right as they leave the center, like the visible
/// edge of a wheel.
fn slot_x(slot: usize, scroll_offset: f32) -> f32 {
    let rows_from_center = CENTER as f32 - slot as f32 + scroll_offset;
    WHEEL_X + BULGE_PER_ROW * rows_from_center * rows_from_center
}

fn slot_entry(wheel: &Wheel, slot: usize) -> Option<&WheelEntry> {
    if wheel.entries.is_empty() {
        return None;
    }
    let len = wheel.entries.len() as i64;
    let index = (wheel.active as i64 + slot as i64 - CENTER as i64).rem_euclid(len);
    wheel.entries.get(index as usize)
}

fn build_entries(library: &StepfileLibrary, expanded: Option<usize>) -> Vec<WheelEntry> {
    let mut entries = Vec::new();
    for (group_index, group) in library.groups.iter().enumerate() {
        let is_expanded = expanded == Some(group_index);
        entries.push(WheelEntry::Group {
            index: group_index,
            label: format!("{} ({})", group.name, group.stepfiles.len()),
        });
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
        .filter(|(_, chart)| chart.steps_type == StepsType::DanceSingle && !chart.notes.is_empty())
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

fn difficulty_style(difficulty: &Difficulty) -> (&str, Color) {
    match difficulty {
        Difficulty::Beginner => ("Beginner", Color::srgb(0.35, 0.9, 0.95)),
        Difficulty::Easy => ("Basic", Color::srgb(0.95, 0.8, 0.25)),
        Difficulty::Medium => ("Difficult", Color::srgb(0.95, 0.35, 0.3)),
        Difficulty::Hard => ("Expert", Color::srgb(0.4, 0.95, 0.4)),
        Difficulty::Challenge => ("Challenge", Color::srgb(0.8, 0.45, 0.95)),
        Difficulty::Edit => ("Edit", Color::srgb(0.7, 0.7, 0.75)),
        Difficulty::Other(name) => (name.as_str(), Color::srgb(0.7, 0.7, 0.75)),
    }
}

fn bpm_label(stepfile: &Stepfile) -> String {
    match stepfile.display_bpm {
        Some(DisplayBpm::Single(bpm)) => format!("BPM {bpm:.0}"),
        Some(DisplayBpm::Range(low, high)) => format!("BPM {low:.0}-{high:.0}"),
        Some(DisplayBpm::Random) => "BPM ???".to_string(),
        None => {
            let (low, high) = stepfile.timing.bpm_range();
            if (high - low).abs() < 0.5 {
                format!("BPM {low:.0}")
            } else {
                format!("BPM {low:.0}-{high:.0}")
            }
        }
    }
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
