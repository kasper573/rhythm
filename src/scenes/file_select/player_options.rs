use super::{FileSelectMode, STATS_TEXT, STEPFILE_TEXT};
use crate::core::at;
use crate::core::config::GameConfig;
use crate::core::font::game_font;
use crate::core::input::{Actions, GameAction, NavPulse};
use crate::core::menu::{ACTIVE_COLOR, INACTIVE_COLOR, TITLE_COLOR};
use crate::core::note_field::NoteSpeed;
use crate::core::note_skin::NoteSkinLibrary;
use crate::core::scene_flow::SpawnScoped;
use crate::core::settings::Settings;
use crate::core::sfx::{PlaySfx, Sfx};
use crate::scenes::GameScene;
use bevy::ecs::query::QueryFilter;
use bevy::prelude::*;
use strum::{EnumCount, EnumIter, IntoEnumIterator, IntoStaticStr};

/// The player options modal: edits the stepfile options in place (they live
/// in the settings, so changes persist immediately) as an edge-to-edge
/// stripe over the vertical center of the file select, which stays mounted
/// underneath. Also keeps the options summary next to the wheel current.
pub(super) fn plugin(app: &mut App) {
    app.add_systems(OnEnter(FileSelectMode::PlayerOptions), enter)
        .add_systems(
            Update,
            refresh_summary.run_if(in_state(GameScene::FileSelect)),
        )
        .add_systems(
            Update,
            (
                handle_pulses,
                handle_close,
                rebuild_value_lists,
                highlight_rows,
                animate_underline,
                animate_transition,
            )
                .chain()
                .run_if(in_state(FileSelectMode::PlayerOptions)),
        );
}

/// The background and the content are siblings so the transition can slide
/// them in from opposite directions.
fn enter(
    mut commands: Commands,
    settings: Res<Settings>,
    config: Res<GameConfig>,
    skins: Res<NoteSkinLibrary>,
) {
    let rows: Vec<_> = (0..OptionRow::COUNT)
        .map(|index| {
            let (values, selected) = row_values(row(index), &settings, &config, &skins);
            let label: &str = row(index).into();
            let values = value_scenes(index, values);
            bsn! {
                Node { column_gap: px(24), align_items: AlignItems::Center }
                Children [
                    (
                        Node { width: px(240) }
                        Children [(
                            RowText(index)
                            game_font(28.0)
                            Text({label.to_string()})
                            TextColor({INACTIVE_COLOR})
                        )]
                    ),
                    (
                        RowValues(index)
                        Node { column_gap: px(22), align_items: AlignItems::Center }
                        Children [ {values} ]
                    ),
                    (
                        Underline { row: index, target: selected }
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(0),
                            bottom: px(-2),
                            width: px(0),
                            height: px(4),
                            border_radius: {BorderRadius::all(Val::Px(2.0))},
                        }
                        BackgroundColor({STEPFILE_TEXT})
                    ),
                ]
            }
        })
        .collect();
    commands.spawn_scoped(
        FileSelectMode::PlayerOptions,
        bsn! {
            ActiveRow(0)
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
                        Node {
                            position_type: PositionType::Absolute,
                            left: percent(-100),
                            top: px(0),
                            bottom: px(0),
                            width: percent(100),
                        }
                        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0))
                    ),
                    (
                        ModalContent
                        Node {
                            width: percent(100),
                            left: percent(100),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            padding: {UiRect::vertical(Val::Px(28.0))},
                            row_gap: px(14),
                        }
                        Children [
                            (
                                game_font(52.0)
                                Text("Player Options")
                                TextColor({TITLE_COLOR})
                                Node { margin: {UiRect::bottom(Val::Px(24.0))} }
                            ),
                            {rows},
                        ]
                    ),
                ]
            )]
        },
    );
}

fn value_scenes(row: usize, values: Vec<String>) -> Vec<impl Scene> {
    values
        .into_iter()
        .enumerate()
        .map(move |(index, value)| {
            bsn! {
                ValueText { row: row, index: index }
                game_font(28.0)
                Text({value})
                TextColor({INACTIVE_COLOR})
            }
        })
        .collect()
}

fn handle_pulses(
    mut pulses: MessageReader<NavPulse>,
    modal: Single<&ModalTransition>,
    mut active: Single<&mut ActiveRow>,
    mut settings: ResMut<Settings>,
    config: Res<GameConfig>,
    skins: Res<NoteSkinLibrary>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    if modal.dir < 0.0 {
        return;
    }
    for pulse in pulses.read() {
        match pulse.action {
            GameAction::Previous => {
                active.0 = (active.0 + OptionRow::COUNT - 1) % OptionRow::COUNT;
                sfx.write(PlaySfx(Sfx::Navigate));
            }
            GameAction::Next => {
                active.0 = (active.0 + 1) % OptionRow::COUNT;
                sfx.write(PlaySfx(Sfx::Navigate));
            }
            GameAction::Left | GameAction::Right => {
                let delta = if pulse.action == GameAction::Left {
                    -1
                } else {
                    1
                };
                if change_value(row(active.0), delta, &mut settings, &config, &skins) {
                    sfx.write(PlaySfx(Sfx::Navigate));
                }
            }
            _ => {}
        }
    }
}

fn handle_close(
    actions: Actions,
    mut modal: Single<&mut ModalTransition>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    if modal.dir < 0.0 {
        return;
    }
    if actions.just_pressed(GameAction::Cancel) || actions.just_pressed(GameAction::Select) {
        sfx.write(PlaySfx(Sfx::Cancel));
        modal.dir = -1.0;
    }
}

/// The type and skin lists never change after enter; only the modifier
/// list does, and only when the speed type flips. That one respawn happens
/// in a single command batch — a queued spawn would leave the row empty
/// for a frame and make the centered layout jump.
fn rebuild_value_lists(
    settings: Res<Settings>,
    config: Res<GameConfig>,
    skins: Res<NoteSkinLibrary>,
    containers: Query<(Entity, &RowValues)>,
    mut underlines: Query<&mut Underline>,
    mut last_dynamic: Local<Option<bool>>,
    mut commands: Commands,
) {
    if !settings.is_changed() {
        return;
    }
    for (_, marker) in &containers {
        let (_, selected) = row_values(row(marker.0), &settings, &config, &skins);
        for mut underline in &mut underlines {
            if underline.row == marker.0 {
                underline.target = selected;
            }
        }
    }

    let dynamic = matches!(settings.stepfile.note_speed, NoteSpeed::Dynamic(_));
    let type_changed = last_dynamic.is_some_and(|last| last != dynamic);
    *last_dynamic = Some(dynamic);
    if !type_changed {
        return;
    }
    for (container, marker) in &containers {
        if row(marker.0) != OptionRow::SpeedModifier {
            continue;
        }
        let (values, _) = row_values(row(marker.0), &settings, &config, &skins);
        commands.entity(container).despawn_related::<Children>();
        for scene in value_scenes(marker.0, values) {
            commands.spawn_scene(scene).insert(ChildOf(container));
        }
    }
}

fn highlight_rows(
    active: Single<&ActiveRow>,
    mut labels: Query<(&RowText, &mut TextColor)>,
    mut values: Query<(&ValueText, &mut TextColor), Without<RowText>>,
) {
    for (label, mut color) in &mut labels {
        let wanted = row_color(label.0 == active.0);
        if color.0 != wanted {
            color.0 = wanted;
        }
    }
    for (value, mut color) in &mut values {
        let wanted = row_color(value.row == active.0);
        if color.0 != wanted {
            color.0 = wanted;
        }
    }
}

const UNDERLINE_RATE: f32 = 14.0;

/// Slides and resizes each row's underline toward its selected value,
/// measuring the laid-out text so the tween tracks real glyph widths.
fn animate_underline(
    time: Res<Time>,
    values: Query<(&ValueText, &ComputedNode, &UiGlobalTransform)>,
    rows: Query<(&ComputedNode, &UiGlobalTransform)>,
    mut underlines: Query<(&mut Underline, &mut Node, &ChildOf)>,
) {
    for (mut underline, mut node, child_of) in &mut underlines {
        let Ok((row_node, row_transform)) = rows.get(child_of.parent()) else {
            continue;
        };
        let Some((_, value_node, value_transform)) = values
            .iter()
            .find(|(value, ..)| value.row == underline.row && value.index == underline.target)
        else {
            continue;
        };
        if value_node.size.x == 0.0 {
            continue;
        }
        let scale = value_node.inverse_scale_factor;
        let left = (value_transform.translation.x
            - value_node.size.x / 2.0
            - (row_transform.translation.x - row_node.size.x / 2.0))
            * scale;
        let target = Vec2::new(left, value_node.size.x * scale);
        let current = match underline.current {
            None => target,
            Some(current) => {
                current + (target - current) * (1.0 - (-UNDERLINE_RATE * time.delta_secs()).exp())
            }
        };
        underline.current = Some(current);
        node.left = Val::Px(current.x);
        node.width = Val::Px(current.y);
    }
}

#[derive(QueryFilter)]
struct ContentOnly {
    _content: With<ModalContent>,
    _not_background: Without<ModalBackground>,
}

#[derive(QueryFilter)]
struct UnderlineFill {
    _underline: With<Underline>,
    _not_background: Without<ModalBackground>,
}

/// The background slides in from the left and the content from the right,
/// both fading in; closing plays the same effect in reverse and only then
/// leaves the modal state.
fn animate_transition(
    time: Res<Time>,
    mut mode: ResMut<NextState<FileSelectMode>>,
    mut modal: Single<&mut ModalTransition>,
    mut background: Single<(&mut Node, &mut BackgroundColor), With<ModalBackground>>,
    mut content: Single<&mut Node, ContentOnly>,
    mut texts: Query<&mut TextColor, With<Node>>,
    mut underlines: Query<&mut BackgroundColor, UnderlineFill>,
) {
    if modal.t >= 1.0 && modal.dir > 0.0 {
        return;
    }
    modal.t = (modal.t + modal.dir * time.delta_secs() / TRANSITION_SECONDS).clamp(0.0, 1.0);
    if modal.t <= 0.0 && modal.dir < 0.0 {
        mode.set(FileSelectMode::Browse);
    }
    let eased = EaseFunction::CubicOut.sample_clamped(modal.t);
    let (background_node, background_color) = &mut *background;
    background_node.left = Val::Percent(-100.0 * (1.0 - eased));
    background_color.0 = Color::srgba(0.0, 0.0, 0.0, eased);
    content.left = Val::Percent(100.0 * (1.0 - eased));
    for mut color in &mut texts {
        color.0.set_alpha(eased);
    }
    for mut color in &mut underlines {
        color.0.set_alpha(eased);
    }
}

/// Upserts the one-line options summary shown next to the wheel, e.g.
/// `Dynamic 2x · DDREx Note`.
fn refresh_summary(
    settings: Res<Settings>,
    skins: Res<NoteSkinLibrary>,
    text: Option<Single<&mut Text2d, With<OptionsSummary>>>,
    mut commands: Commands,
) {
    let Some(mut text) = text else {
        let line = summary(&settings, &skins);
        commands.spawn_scoped(
            GameScene::FileSelect,
            bsn! {
                OptionsSummary
                game_font(22.0)
                Text2d({line})
                TextColor({STATS_TEXT})
                at(-320.0, -150.0, 5.0)
            },
        );
        return;
    };
    if settings.is_changed() {
        text.0 = summary(&settings, &skins);
    }
}

fn summary(settings: &Settings, skins: &NoteSkinLibrary) -> String {
    let options = &settings.stepfile;
    let speed = match options.note_speed {
        NoteSpeed::Constant(value) => {
            format!("Constant {}", format_modifier(value, options.note_speed))
        }
        NoteSpeed::Dynamic(value) => {
            format!("Dynamic {}", format_modifier(value, options.note_speed))
        }
    };
    let skin = skins
        .skins
        .iter()
        .find(|skin| skin.name == options.note_skin)
        .map(|skin| skin.display_name.clone())
        .unwrap_or_else(|| options.note_skin.clone());
    format!("{speed} · {skin}")
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
}

fn row(index: usize) -> OptionRow {
    OptionRow::iter().nth(index).expect("row index is wrapped")
}

#[derive(Component, Clone, FromTemplate)]
struct ActiveRow(usize);

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

#[derive(Component, Default, Clone)]
struct ModalContent;

#[derive(Component, Clone, FromTemplate)]
struct RowText(usize);

#[derive(Component, Clone, FromTemplate)]
struct RowValues(usize);

#[derive(Component, Clone, FromTemplate)]
struct ValueText {
    row: usize,
    index: usize,
}

/// One per option row; [`animate_underline`] tweens it to the selected value.
#[derive(Component, Clone, FromTemplate)]
struct Underline {
    row: usize,
    target: usize,
    current: Option<Vec2>,
}

#[derive(Component, Default, Clone)]
struct OptionsSummary;

/// Steps the row's value; the ends do not wrap. Switching the speed type
/// resets the modifier to the new type's default — they are one value in
/// reality (see [`NoteSpeed`]).
fn change_value(
    row: OptionRow,
    delta: i32,
    settings: &mut Settings,
    config: &GameConfig,
    skins: &NoteSkinLibrary,
) -> bool {
    let options = &mut settings.stepfile;
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
    }
}

/// The row's value labels and which one is selected.
fn row_values(
    row: OptionRow,
    settings: &Settings,
    config: &GameConfig,
    skins: &NoteSkinLibrary,
) -> (Vec<String>, usize) {
    let options = &settings.stepfile;
    match row {
        OptionRow::SpeedType => {
            let selected = match options.note_speed {
                NoteSpeed::Constant(_) => 0,
                NoteSpeed::Dynamic(_) => 1,
            };
            (
                vec!["Constant".to_string(), "Dynamic".to_string()],
                selected,
            )
        }
        OptionRow::SpeedModifier => {
            let set = config.speed_modifiers.set(options.note_speed);
            (
                set.options
                    .iter()
                    .map(|value| format_modifier(*value, options.note_speed))
                    .collect(),
                selected_index(&set.options, options.note_speed.value()),
            )
        }
        OptionRow::NoteSkin => {
            let selected = skins
                .skins
                .iter()
                .position(|skin| skin.name == options.note_skin)
                .unwrap_or(0);
            (
                skins
                    .skins
                    .iter()
                    .map(|skin| skin.display_name.clone())
                    .collect(),
                selected,
            )
        }
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
