use super::{FileSelectFocus, ModalStripe};
use crate::core::SCREEN_SIZE;
use crate::core::config::GameConfig;
use crate::core::font::game_font;
use crate::core::input::{Actions, GameAction, NavPulse, StepDirection};
use crate::core::menu::{ACTIVE_COLOR, INACTIVE_COLOR, TITLE_COLOR};
use crate::core::note_field::{
    LaneView, NoteField, NoteFieldClock, NoteSpeed, Perspective, fitted_arrow_size, max_arrow_size,
};
use crate::core::note_skin::{ActiveNoteSkins, NoteSkinLibrary};
use crate::core::player::{PlayMode, PlayerId};
use crate::core::scene_flow::SpawnScoped;
use crate::core::settings::{GradeLayer, MachineSettings, PlayerOptions, PlayerSettings};
use crate::core::sfx::{PlaySfx, Sfx};
use crate::core::stepfile::{Arrow, MusicPlayer, Row, StepfileTiming, Tail};
use crate::core::units::{Beat, Percent, Seconds};
use crate::scenes::file_player::{
    FieldSpec, GameplayDrive, PlayInput, PlayTime, PrefabAssets, SessionSpec, clear_session,
    grade_text, spawn_session,
};
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, RenderTarget, ScalingMode};
use bevy::ecs::query::QueryFilter;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use strum::{EnumCount, EnumIter, IntoEnumIterator, IntoStaticStr};

/// The player options modal: edits each active player's options in place
/// (they live in the player settings, so changes persist immediately) as
/// an edge-to-edge stripe over the vertical center of the file select,
/// which stays mounted underneath. One options panel per active player —
/// P1 to the left, P2 to the right — each driven by its player's own pad.
pub(super) fn plugin(app: &mut App) {
    app.add_systems(OnEnter(FileSelectFocus::PlayerOptions), enter)
        .add_systems(OnExit(FileSelectFocus::PlayerOptions), teardown_preview)
        .add_systems(
            Update,
            (
                handle_pulses,
                handle_close,
                refresh_values,
                highlight_rows,
                animate_transition,
            )
                .chain()
                .run_if(in_state(FileSelectFocus::PlayerOptions)),
        )
        .add_systems(
            Update,
            build_preview.run_if(
                in_state(FileSelectFocus::PlayerOptions)
                    .and_then(not(resource_exists::<PreviewState>)),
            ),
        )
        .add_systems(
            Update,
            rebuild_preview.run_if(
                in_state(FileSelectFocus::PlayerOptions).and_then(resource_exists::<PreviewState>),
            ),
        )
        // The mocked adapter's port drivers: fill the wheel-music clock and the
        // deterministic autoplay input, in the prefab's drive phase.
        .add_systems(
            Update,
            (drive_clock, mock_input)
                .chain()
                .in_set(GameplayDrive)
                .run_if(resource_exists::<PreviewState>),
        );
}

/// The background and the content are siblings so the transition can slide
/// them in from opposite directions.
fn enter(
    mut commands: Commands,
    mode: Res<PlayMode>,
    settings: Res<PlayerSettings>,
    config: Res<GameConfig>,
    skins: Res<NoteSkinLibrary>,
) {
    let players = mode.players();
    let versus = players.len() > 1;
    // Each player's cursor lives on its own entity; the shared table reads
    // them all so option names render once and the value columns line up.
    for player in players {
        commands.spawn_scoped(
            FileSelectFocus::PlayerOptions,
            bsn! { OptionsPanel { player: {*player}, active_row: 0 } },
        );
    }
    // Equal flanks on both sides keep the table centered whatever it reads;
    // each active player's flank hosts a preview surface the mocked adapter
    // fills through the file player prefab (see below).
    let left = vec![flank_scene(players.first().copied())];
    let options = vec![options_column_scene(
        players, versus, &settings, &config, &skins,
    )];
    let right = vec![flank_scene(players.get(1).copied())];
    commands.spawn_scoped(
        FileSelectFocus::PlayerOptions,
        bsn! {
            ModalTransition { t: 0.0, dir: 1.0 }
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
            }
            Children [(
                Node { width: percent(100) }
                Children [
                    (
                        ModalBackground
                        ModalStripe
                        Node {
                            position_type: PositionType::Absolute,
                            left: percent(-100),
                            top: px(0),
                            bottom: px(0),
                            width: percent(100),
                            overflow: {Overflow::clip()},
                        }
                        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0))
                    ),
                    (
                        ModalContent
                        Node {
                            width: percent(100),
                            left: percent(100),
                            flex_direction: FlexDirection::Row,
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            padding: {UiRect::vertical(Val::Px(24.0))},
                        }
                        Children [ {left}, {options}, {right} ]
                    ),
                ]
            )]
        },
    );
}

/// A flex-grow column beside the table. Present on both sides so the table
/// stays centered; an active player's flank carries a [`PreviewSurface`] the
/// file player renders their autoplayed field into, an empty one is a spacer.
fn flank_scene(player: Option<PlayerId>) -> impl Scene {
    let surface: Vec<_> = player.map(surface_scene).into_iter().collect();
    bsn! {
        Node {
            flex_grow: 1.0,
            flex_basis: px(0),
            min_width: px(0),
            align_self: AlignSelf::Stretch,
        }
        Children [ {surface} ]
    }
}

fn surface_scene(player: PlayerId) -> impl Scene {
    bsn! {
        PreviewSurface { player: {player} }
        Node {
            width: percent(100),
            height: percent(100),
            overflow: {Overflow::clip()},
        }
    }
}

// ===== The mocked gameplay adapter =====
// Drives the file player prefab (see `file_player::spawn_session`) as an
// autoplayed preview: a mocked chart on offscreen surfaces, clocked by the
// wheel music, rebuilt in place whenever an option changes so the preview
// always reflects the current selection exactly.

/// The UI node a player's preview renders into; the adapter fills it with the
/// blitted playfield image.
#[derive(Component, Clone, Copy, FromTemplate)]
struct PreviewSurface {
    player: PlayerId,
}

/// The design-canvas height each preview frames, so a field reads as tall as
/// it does full-screen before its surface scales it down.
const PREVIEW_BAND: f32 = SCREEN_SIZE.y;
/// Per-surface lane index, so each field draws on its own layer/camera.
const PREVIEW_LANE_BASE: usize = 40;
/// The 2D grade cameras' orders, bracketing the lane cameras (orders `1 +
/// lane`, i.e. 41 up): behind clears each image to the stripe black so the
/// additive glow lands on it, in front draws last.
const PREVIEW_BEHIND_ORDER: isize = 20;
const PREVIEW_FRONT_ORDER: isize = 60;
/// The grades the autoplay walks through — only the top three tiers.
const PREVIEW_GRADES: [usize; 3] = [0, 1, 2];

/// Marks every field entity the preview spawns, so a rebuild can clear just
/// the fields and leave the surfaces.
#[derive(Component, Clone)]
struct PreviewField;

/// The live preview: its per-player surfaces, the mocked chart, and the music
/// loop tracking that triggers a restart.
#[derive(Resource)]
struct PreviewState {
    surfaces: Vec<PreviewSurfaceInfo>,
    timing: StepfileTiming,
    rows: Vec<Row>,
    last_visible: Seconds,
    /// Set on the first frame and each music loop; consumed by the rebuild.
    rebuild: bool,
}

struct PreviewSurfaceInfo {
    player: PlayerId,
    image: Handle<Image>,
    index: usize,
}

/// The prefab's asset handles, bundled for the spawners.
#[derive(SystemParam)]
struct PreviewAssets<'w> {
    asset_server: Res<'w, AssetServer>,
    images: ResMut<'w, Assets<Image>>,
    config: Res<'w, GameConfig>,
    skins: Res<'w, ActiveNoteSkins>,
}

impl PreviewAssets<'_> {
    fn prefab(&mut self) -> PrefabAssets<'_> {
        PrefabAssets {
            asset_server: &self.asset_server,
            images: &mut self.images,
            config: &self.config,
            skins: &self.skins,
        }
    }
}

/// Once the wheel music plays and every surface is laid out, builds a render
/// image and its compositing cameras per surface, publishes the band's clock
/// and grade area, and arms the first field spawn (via [`rebuild_preview`]).
fn build_preview(
    music: Res<MusicPlayer>,
    config: Res<GameConfig>,
    surfaces: Query<(Entity, &PreviewSurface, &ComputedNode)>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
) {
    let Some((timing, start, length)) = music.loop_window() else {
        return;
    };
    let mut sized: Vec<(Entity, PlayerId, Vec2)> = surfaces
        .iter()
        .map(|(node, surface, computed)| (node, surface.player, computed.size()))
        .collect();
    if sized.is_empty()
        || sized
            .iter()
            .any(|(_, _, size)| size.x <= 0.0 || size.y <= 0.0)
    {
        return;
    }
    sized.sort_by_key(|(_, player, _)| player_order(*player));

    let mut infos = Vec::new();
    for (index, (node, player, size)) in sized.into_iter().enumerate() {
        let resolution = image_size(size);
        let image = images.add(Image::new_target_texture(
            resolution.x,
            resolution.y,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
        let (behind, front) = surface_layers(index);
        for (order, layer, clear) in [
            (
                PREVIEW_BEHIND_ORDER,
                behind,
                ClearColorConfig::Custom(Color::BLACK),
            ),
            (PREVIEW_FRONT_ORDER, front, ClearColorConfig::None),
        ] {
            commands.spawn_scene(bsn! { Camera2d }).insert((
                Camera {
                    order,
                    clear_color: clear,
                    ..default()
                },
                RenderTarget::Image(image.clone().into()),
                band_projection(),
                RenderLayers::layer(layer),
                DespawnOnExit(FileSelectFocus::PlayerOptions),
            ));
        }
        commands.entity(node).insert(ImageNode::new(image.clone()));
        infos.push(PreviewSurfaceInfo {
            player,
            image,
            index,
        });
    }

    let half = PREVIEW_BAND / 2.0;
    let padding = config.stage.screen_edge_padding;
    let arrow = preview_arrow_size(&config);
    commands.insert_resource(NoteFieldClock {
        visible: Seconds::ZERO,
        timing: timing.clone(),
        target_y: half - padding - arrow / 2.0,
    });
    commands.insert_resource(grade_text::grade_area(half - padding, -half + padding));
    commands.insert_resource(PreviewState {
        surfaces: infos,
        rows: mocked_rows(&timing, start, length),
        timing,
        last_visible: Seconds::ZERO,
        rebuild: true,
    });
}

/// The mocked adapter's CLOCK driver: fills the prefab's [`PlayTime`] port
/// from the wheel music and flags a rebuild each time the music loops back.
fn drive_clock(
    music: Res<MusicPlayer>,
    machine: Res<MachineSettings>,
    mut state: ResMut<PreviewState>,
    mut play_time: ResMut<PlayTime>,
) {
    let Some((visible, _)) = music.visible_now(&machine.timing) else {
        return;
    };
    if visible.0 + 0.05 < state.last_visible.0 {
        state.rebuild = true;
    }
    state.last_visible = visible;
    play_time.graded = visible;
    play_time.visible = visible;
}

/// The mocked adapter's INPUT driver: fills the prefab's [`PlayInput`] port
/// deterministically — every note in its hit window is pressed at the offset
/// that grades it to its tier, held through a hold's tail. No keyboard.
fn mock_input(
    state: Res<PreviewState>,
    play_time: Res<PlayTime>,
    config: Res<GameConfig>,
    mut input: ResMut<PlayInput>,
) {
    input.clear();
    let now = play_time.graded.0;
    let timing = &state.timing;
    for surface in &state.surfaces {
        for row in &state.rows {
            let time = timing.seconds_at_beat(row.beat).0;
            let Some(offset) = autoplay_offset(&config, note_tier(row.beat)) else {
                continue;
            };
            let due = time - offset.0;
            for arrow in &row.arrows {
                if now >= due && now < arrow_until(row, arrow, timing) {
                    let action =
                        GameAction::step(surface.player, StepDirection::of_column(arrow.column));
                    input.press(action, true);
                }
            }
        }
    }
}

/// Rebuilds the fields whenever an option changes or the music loops, so the
/// preview always reflects the current selection exactly. Only rows still
/// within their hit window are laid down, so a mid-stream rebuild never
/// re-materializes a passed note that would then miss.
fn rebuild_preview(
    mut state: ResMut<PreviewState>,
    settings: Res<PlayerSettings>,
    music: Res<MusicPlayer>,
    machine: Res<MachineSettings>,
    mut assets: PreviewAssets,
    fields: Query<Entity, With<PreviewField>>,
    mut commands: Commands,
) {
    if !state.rebuild && !settings.is_changed() && !assets.skins.is_changed() {
        return;
    }
    state.rebuild = false;
    for entity in &fields {
        commands.entity(entity).despawn();
    }
    let now = music
        .visible_now(&machine.timing)
        .map(|(visible, _)| visible.0)
        .unwrap_or(f64::NEG_INFINITY);
    let timing = state.timing.clone();
    let live: Vec<Row> = state
        .rows
        .iter()
        .filter(|row| row_until(row, &timing) > now)
        .cloned()
        .collect();
    let specs: Vec<FieldSpec> = state
        .surfaces
        .iter()
        .map(|surface| {
            let (behind, front) = surface_layers(surface.index);
            let present = match settings[surface.player].grade_layer {
                GradeLayer::Behind => behind,
                GradeLayer::InFront => front,
            };
            FieldSpec {
                layout: NoteField {
                    player: surface.player,
                    lane: PREVIEW_LANE_BASE + surface.index,
                    origin_x: 0.0,
                    columns: 4,
                    speed: settings[surface.player].note_speed,
                    arrow_size: preview_arrow_size(&assets.config),
                    view: band_view(surface.image.clone()),
                },
                rows: &live,
                mines: &[],
                grade_source_layer: behind_source(surface.index),
                grade_present_layer: Some(present),
                max_health: u32::MAX,
            }
        })
        .collect();
    spawn_session(
        &mut commands,
        &mut assets.prefab(),
        SessionSpec {
            title: String::new(),
            fields: specs,
            timing: state.timing.clone(),
        },
        (DespawnOnExit(FileSelectFocus::PlayerOptions), PreviewField),
    );
}

/// Leaves the modal: drops the preview's ports (its entities and surfaces are
/// scoped to the modal and despawn on their own).
fn teardown_preview(mut commands: Commands) {
    clear_session(&mut commands);
    commands.remove_resource::<PreviewState>();
    commands.remove_resource::<NoteFieldClock>();
    commands.remove_resource::<grade_text::GradeArea>();
}

/// How long past its note an autoplayed tap stays pressed — long enough to
/// bank, short enough not to catch the next note in its column.
const AUTOPLAY_TAP_HOLD: f64 = 0.05;

/// The grade tier a note plays to, by its 8th-note position so it stays put
/// however the rows are filtered on a rebuild.
fn note_tier(beat: Beat) -> usize {
    let ordinal = (beat.0 * 2.0).round() as i64;
    PREVIEW_GRADES[ordinal.rem_euclid(PREVIEW_GRADES.len() as i64) as usize]
}

/// The seconds an arrow stops being pressed: a hold's tail, or a tap's brief
/// release window.
fn arrow_until(row: &Row, arrow: &Arrow, timing: &StepfileTiming) -> f64 {
    match arrow.tail {
        Some(tail) => timing.seconds_at_beat(tail.end).0,
        None => timing.seconds_at_beat(row.beat).0 + AUTOPLAY_TAP_HOLD,
    }
}

/// The seconds a row stops being pressable, for the rebuild's live-rows filter.
fn row_until(row: &Row, timing: &StepfileTiming) -> f64 {
    row.arrows
        .iter()
        .map(|arrow| arrow_until(row, arrow, timing))
        .fold(timing.seconds_at_beat(row.beat).0, f64::max)
}

/// The timing error to press a note with so it grades to `tier`: the midpoint
/// of the tier's window.
fn autoplay_offset(config: &GameConfig, tier: usize) -> Option<Seconds> {
    let dynamic = &config.grading.dynamic;
    let def = dynamic.get(tier)?;
    let lower = if tier == 0 {
        0.0
    } else {
        dynamic[tier - 1].window_ms
    };
    Some(Seconds::from_millis((lower + def.window_ms) / 2.0))
}

fn player_order(player: PlayerId) -> u8 {
    match player {
        PlayerId::P1 => 0,
        PlayerId::P2 => 1,
    }
}

/// The behind/in-front present layers surface `index` draws on, clear of the
/// file-select scene's (0, 8) and of every other surface.
fn surface_layers(index: usize) -> (usize, usize) {
    let base = 24 + index * 4;
    (base + 1, base + 2)
}

/// The offscreen grade word's private source layer for surface `index`.
fn behind_source(index: usize) -> usize {
    24 + index * 4
}

/// The lane camera's view onto a surface image: full [`PREVIEW_BAND`] tall,
/// centered, its width following the image's aspect.
fn band_view(image: Handle<Image>) -> LaneView {
    LaneView {
        target: RenderTarget::Image(image.into()),
        canvas: Vec2::new(1.0, PREVIEW_BAND),
    }
}

/// The 2D grade cameras' projection, matching the lane camera's framing.
fn band_projection() -> Projection {
    Projection::Orthographic(OrthographicProjection {
        scaling_mode: ScalingMode::AutoMin {
            min_width: 1.0,
            min_height: PREVIEW_BAND,
        },
        ..OrthographicProjection::default_2d()
    })
}

/// A surface's render image size: the node's pixels, capped so a hi-dpi node
/// stays a sane texture, aspect preserved so the blit never distorts.
fn image_size(node: Vec2) -> UVec2 {
    const MAX_DIM: f32 = 2048.0;
    let scale = (MAX_DIM / node.x.max(node.y).max(1.0)).min(1.0);
    UVec2::new(
        ((node.x * scale).round() as u32).max(1),
        ((node.y * scale).round() as u32).max(1),
    )
}

/// A single field's arrows at the file player's design-canvas size, so the
/// preview reads as a shrunk file player.
fn preview_arrow_size(config: &GameConfig) -> f32 {
    fitted_arrow_size(
        4.0,
        SCREEN_SIZE.x - 2.0 * config.stage.margin_x,
        max_arrow_size(config, None),
    )
}

/// The mocked chart: `U, L, D-hold, U, R, D-hold` per measure — a mix of 4th
/// and 8th notes with two short holds — repeated across the music's loop.
fn mocked_rows(timing: &StepfileTiming, start: Seconds, length: Seconds) -> Vec<Row> {
    // (beat within the measure, column L=0/D=1/U=2/R=3, hold end beat, quant).
    const PATTERN: [(f64, usize, Option<f64>, u32); 6] = [
        (0.0, 2, None, 4),
        (0.5, 0, None, 8),
        (1.0, 1, Some(1.5), 4),
        (2.0, 2, None, 4),
        (2.5, 3, None, 8),
        (3.0, 1, Some(3.5), 4),
    ];
    let first = (timing.beat_at_seconds(start).0 / 4.0).ceil() as i64;
    let last = ((timing.beat_at_seconds(start + length).0 / 4.0).floor() as i64 - 1).max(first);
    (first..=last)
        .flat_map(|measure| {
            let base = measure as f64 * 4.0;
            PATTERN
                .iter()
                .map(move |&(offset, column, hold_end, quant)| Row {
                    beat: Beat(base + offset),
                    quant,
                    arrows: vec![Arrow {
                        column,
                        tail: hold_end.map(|end| Tail {
                            end: Beat(base + end),
                            roll: false,
                        }),
                    }],
                })
        })
        .collect()
}

/// The options table: the title, an optional P1/P2 header row, then one row
/// per option — its name once, then a value cell per active player.
fn options_column_scene(
    players: &[PlayerId],
    versus: bool,
    settings: &PlayerSettings,
    config: &GameConfig,
    skins: &NoteSkinLibrary,
) -> impl Scene {
    let header: Vec<_> = versus
        .then(|| header_row_scene(players))
        .into_iter()
        .collect();
    let rows: Vec<_> = OptionRow::iter()
        .enumerate()
        .map(|(index, row)| option_row_scene(index, row, players, settings, config, skins))
        .collect();
    bsn! {
        Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(12),
            flex_shrink: 0.0,
        }
        Children [
            (
                ModalText
                game_font(48.0)
                Text("Player Options")
                TextColor({TITLE_COLOR})
                Node { margin: {UiRect::bottom(Val::Px(12.0))} }
            ),
            {header},
            {rows},
        ]
    }
}

/// The `P1`/`P2` tags over the value columns (versus only).
fn header_row_scene(players: &[PlayerId]) -> impl Scene {
    let tags: Vec<_> = players
        .iter()
        .map(|player| {
            let tag = player.label().to_string();
            bsn! {
                Node { width: px(VALUE_WIDTH), justify_content: JustifyContent::Center }
                Children [(
                    ModalText game_font(30.0) Text({tag}) TextColor({TITLE_COLOR})
                )]
            }
        })
        .collect();
    bsn! {
        Node { flex_direction: FlexDirection::Row, column_gap: px(20) }
        Children [
            (Node { width: px(NAME_WIDTH) }),
            {tags},
        ]
    }
}

/// One option's row: its name (shown once) and a value cell per player.
fn option_row_scene(
    index: usize,
    option: OptionRow,
    players: &[PlayerId],
    settings: &PlayerSettings,
    config: &GameConfig,
    skins: &NoteSkinLibrary,
) -> impl Scene {
    let name = <&str>::from(option).to_string();
    let cells: Vec<_> = players
        .iter()
        .map(|player| {
            let value = row_value(option, &settings[*player], config, skins);
            bsn! {
                Node { width: px(VALUE_WIDTH), justify_content: JustifyContent::Center }
                Children [(
                    ValueText { player: {*player}, row: index }
                    ModalText game_font(28.0) Text({value}) TextColor({INACTIVE_COLOR})
                )]
            }
        })
        .collect();
    bsn! {
        Node { flex_direction: FlexDirection::Row, column_gap: px(20), align_items: AlignItems::Center }
        Children [
            (
                Node { width: px(NAME_WIDTH) }
                Children [(
                    RowText { row: index }
                    ModalText game_font(28.0) Text({name}) TextColor({INACTIVE_COLOR})
                )]
            ),
            {cells},
        ]
    }
}

/// Fixed column widths keep the table from re-centering when a value's text
/// length changes.
const NAME_WIDTH: f32 = 220.0;
const VALUE_WIDTH: f32 = 200.0;

/// Routes each pulse to the pulsing player's own panel: their pad's
/// up/down moves between rows, left/right steps the value.
fn handle_pulses(
    mut pulses: MessageReader<NavPulse>,
    modal: Single<&ModalTransition>,
    mut panels: Query<&mut OptionsPanel>,
    mut settings: ResMut<PlayerSettings>,
    config: Res<GameConfig>,
    skins: Res<NoteSkinLibrary>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    if modal.dir < 0.0 {
        return;
    }
    for pulse in pulses.read() {
        let Some((player, direction)) = pulse.action.as_step() else {
            continue;
        };
        let Some(mut panel) = panels.iter_mut().find(|panel| panel.player == player) else {
            continue;
        };
        let acted = match direction {
            StepDirection::Up => {
                panel.active_row = (panel.active_row + OptionRow::COUNT - 1) % OptionRow::COUNT;
                true
            }
            StepDirection::Down => {
                panel.active_row = (panel.active_row + 1) % OptionRow::COUNT;
                true
            }
            StepDirection::Left | StepDirection::Right => {
                let delta = if direction == StepDirection::Left {
                    -1
                } else {
                    1
                };
                change_value(
                    row(panel.active_row),
                    delta,
                    &mut settings[player],
                    &config,
                    &skins,
                )
            }
        };
        if acted {
            sfx.write(PlaySfx(Sfx::Navigate));
        }
    }
}

/// Closing the modal is a shared space: any active player's ¤Select¤ or
/// ¤Cancel¤ closes it for everyone.
fn handle_close(
    actions: Actions,
    mode: Res<PlayMode>,
    mut modal: Single<&mut ModalTransition>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    if modal.dir < 0.0 {
        return;
    }
    if actions.any_just_pressed(mode.players(), GameAction::cancel)
        || actions.any_just_pressed(mode.players(), GameAction::select)
    {
        sfx.write(PlaySfx(Sfx::Cancel));
        modal.dir = -1.0;
    }
}

/// Keeps every value text on its player's current selection.
fn refresh_values(
    settings: Res<PlayerSettings>,
    config: Res<GameConfig>,
    skins: Res<NoteSkinLibrary>,
    mut values: Query<(&ValueText, &mut Text)>,
) {
    if !settings.is_changed() {
        return;
    }
    for (value, mut text) in &mut values {
        let current = row_value(row(value.row), &settings[value.player], &config, &skins);
        if text.0 != current {
            text.0 = current;
        }
    }
}

fn highlight_rows(
    panels: Query<&OptionsPanel>,
    mut labels: Query<(&RowText, &mut TextColor)>,
    mut values: Query<(&ValueText, &mut TextColor), Without<RowText>>,
) {
    // The name highlights when any player edits that row; a value only when
    // its own player does.
    for (label, mut color) in &mut labels {
        let active = panels.iter().any(|panel| panel.active_row == label.row);
        let wanted = row_color(active);
        if color.0 != wanted {
            color.0 = wanted;
        }
    }
    for (value, mut color) in &mut values {
        let active = panels
            .iter()
            .any(|panel| panel.player == value.player && panel.active_row == value.row);
        let wanted = row_color(active);
        if color.0 != wanted {
            color.0 = wanted;
        }
    }
}

#[derive(QueryFilter)]
struct ContentOnly {
    _content: With<ModalContent>,
    _not_background: Without<ModalBackground>,
}

/// The background slides in from the left and the content from the right,
/// both fading in; closing plays the same effect in reverse and only then
/// leaves the modal state.
fn animate_transition(
    time: Res<Time>,
    mut mode: ResMut<NextState<FileSelectFocus>>,
    mut modal: Single<&mut ModalTransition>,
    mut background: Single<(&mut Node, &mut BackgroundColor), With<ModalBackground>>,
    mut content: Single<&mut Node, ContentOnly>,
    mut texts: Query<&mut TextColor, With<ModalText>>,
) {
    if modal.t >= 1.0 && modal.dir > 0.0 {
        return;
    }
    modal.t = (modal.t + modal.dir * time.delta_secs() / TRANSITION_SECONDS).clamp(0.0, 1.0);
    if modal.t <= 0.0 && modal.dir < 0.0 {
        mode.set(FileSelectFocus::Browse);
    }
    let eased = EaseFunction::CubicOut.sample_clamped(modal.t);
    let (background_node, background_color) = &mut *background;
    background_node.left = Val::Percent(-100.0 * (1.0 - eased));
    background_color.0 = Color::srgba(0.0, 0.0, 0.0, eased);
    content.left = Val::Percent(100.0 * (1.0 - eased));
    for mut color in &mut texts {
        color.0.set_alpha(eased);
    }
}

fn row_color(active: bool) -> Color {
    if active { ACTIVE_COLOR } else { INACTIVE_COLOR }
}

#[derive(Debug, Clone, Copy, PartialEq, EnumCount, EnumIter, IntoStaticStr)]
enum OptionRow {
    #[strum(serialize = "Speed Type")]
    SpeedType,
    #[strum(serialize = "Speed Modifier")]
    SpeedModifier,
    #[strum(serialize = "Note Skin")]
    NoteSkin,
    Perspective,
    #[strum(serialize = "Grade Layer")]
    GradeLayer,
    #[strum(serialize = "Grade Position")]
    GradePosition,
}

/// The grade position steps between the top and bottom screen edges in
/// tenths, shown as a percentage.
const GRADE_POSITION_STEP: Percent = Percent(10.0);

fn row(index: usize) -> OptionRow {
    OptionRow::iter().nth(index).expect("row index is wrapped")
}

/// One player's panel and which of its rows is active.
#[derive(Component, Clone, FromTemplate)]
struct OptionsPanel {
    player: PlayerId,
    active_row: usize,
}

/// `t` runs 0..=1 through the open/close effect; `dir` is +1 while opening
/// and -1 while closing.
#[derive(Component, Clone, FromTemplate)]
struct ModalTransition {
    t: f32,
    dir: f32,
}

const TRANSITION_SECONDS: f32 = 0.25;

#[derive(Component, Default, Clone)]
struct ModalBackground;

/// Every text inside the modal, faded as one by [`animate_transition`].
#[derive(Component, Default, Clone)]
struct ModalText;

#[derive(Component, Default, Clone)]
struct ModalContent;

#[derive(Component, Clone, FromTemplate)]
struct RowText {
    row: usize,
}

#[derive(Component, Clone, FromTemplate)]
struct ValueText {
    player: PlayerId,
    row: usize,
}

/// Steps the row's value; the ends do not wrap. Switching the speed type
/// resets the modifier to the new type's default — they are one value in
/// reality (see [`NoteSpeed`]).
fn change_value(
    row: OptionRow,
    delta: i32,
    options: &mut PlayerOptions,
    config: &GameConfig,
    skins: &NoteSkinLibrary,
) -> bool {
    match row {
        OptionRow::SpeedType => {
            let switched = match (options.note_speed, delta) {
                (NoteSpeed::Dynamic(_), -1) => {
                    NoteSpeed::Constant(config.speed_modifiers.constant.default)
                }
                (NoteSpeed::Constant(_), 1) => {
                    NoteSpeed::Dynamic(config.speed_modifiers.dynamic.default)
                }
                _ => return false,
            };
            options.note_speed = switched;
            true
        }
        OptionRow::SpeedModifier => {
            let set = config.speed_modifiers.set(options.note_speed);
            let index = selected_index(&set.options, options.note_speed.value());
            let stepped = index.saturating_add_signed(delta as isize);
            let Some(value) = set.options.get(stepped).copied() else {
                return false;
            };
            if stepped == index {
                return false;
            }
            options.note_speed = match options.note_speed {
                NoteSpeed::Constant(_) => NoteSpeed::Constant(value),
                NoteSpeed::Dynamic(_) => NoteSpeed::Dynamic(value),
            };
            true
        }
        OptionRow::NoteSkin => {
            let index = skins
                .skins
                .iter()
                .position(|skin| skin.name == options.note_skin)
                .unwrap_or(0);
            let stepped = index.saturating_add_signed(delta as isize);
            let Some(skin) = skins.skins.get(stepped) else {
                return false;
            };
            if stepped == index {
                return false;
            }
            options.note_skin = skin.name.clone();
            true
        }
        OptionRow::Perspective => {
            let all: Vec<Perspective> = Perspective::iter().collect();
            let index = all
                .iter()
                .position(|perspective| *perspective == options.perspective)
                .unwrap_or(0);
            let stepped = index.saturating_add_signed(delta as isize);
            let Some(perspective) = all.get(stepped) else {
                return false;
            };
            if stepped == index {
                return false;
            }
            options.perspective = *perspective;
            true
        }
        OptionRow::GradeLayer => {
            let all: Vec<GradeLayer> = GradeLayer::iter().collect();
            let index = all
                .iter()
                .position(|layer| *layer == options.grade_layer)
                .unwrap_or(0);
            let stepped = index.saturating_add_signed(delta as isize);
            let Some(layer) = all.get(stepped) else {
                return false;
            };
            if stepped == index {
                return false;
            }
            options.grade_layer = *layer;
            true
        }
        OptionRow::GradePosition => {
            let stepped = Percent(
                (options.grade_position.0 + delta as f32 * GRADE_POSITION_STEP.0).clamp(0.0, 100.0),
            );
            if stepped == options.grade_position {
                return false;
            }
            options.grade_position = stepped;
            true
        }
    }
}

/// The label of the row's currently selected value.
fn row_value(
    row: OptionRow,
    options: &PlayerOptions,
    config: &GameConfig,
    skins: &NoteSkinLibrary,
) -> String {
    match row {
        OptionRow::SpeedType => match options.note_speed {
            NoteSpeed::Constant(_) => "Constant".to_string(),
            NoteSpeed::Dynamic(_) => "Dynamic".to_string(),
        },
        OptionRow::SpeedModifier => {
            let set = config.speed_modifiers.set(options.note_speed);
            let index = selected_index(&set.options, options.note_speed.value());
            format_modifier(set.options[index], options.note_speed)
        }
        OptionRow::NoteSkin => skins
            .skins
            .iter()
            .find(|skin| skin.name == options.note_skin)
            .map(|skin| skin.display_name.clone())
            .unwrap_or_else(|| options.note_skin.clone()),
        OptionRow::Perspective => <&str>::from(options.perspective).to_string(),
        OptionRow::GradeLayer => <&str>::from(options.grade_layer).to_string(),
        OptionRow::GradePosition => format!("{:.0}%", options.grade_position.0),
    }
}

/// The option closest to the current value; exact when the value came from
/// the same list.
fn selected_index(options: &[f32], value: f32) -> usize {
    options
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (*a - value).abs().total_cmp(&(*b - value).abs()))
        .map(|(index, _)| index)
        .unwrap_or(0)
}

/// Dynamic multipliers always render with an `x` suffix.
fn format_modifier(value: f32, speed: NoteSpeed) -> String {
    match speed {
        NoteSpeed::Constant(_) => format_value(value),
        NoteSpeed::Dynamic(_) => format!("{}x", format_value(value)),
    }
}

fn format_value(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}
