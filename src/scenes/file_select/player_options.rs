use super::{FileSelectMode, STATS_TEXT, STEPFILE_TEXT};
use crate::core::config::GameConfig;
use crate::core::font::GameFont;
use crate::core::input::{Actions, GameAction, NavPulse};
use crate::core::menu::{ACTIVE_COLOR, INACTIVE_COLOR, TITLE_COLOR};
use crate::core::note_field::NoteSpeed;
use crate::core::note_skin::NoteSkinLibrary;
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
    font: Res<GameFont>,
    settings: Res<Settings>,
    config: Res<GameConfig>,
    skins: Res<NoteSkinLibrary>,
) {
    commands
        .spawn((
            DespawnOnExit(FileSelectMode::PlayerOptions),
            ActiveRow(0),
            ModalTransition { t: 0.0, dir: 1.0 },
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .with_children(|screen| {
            screen
                .spawn(Node {
                    width: Val::Percent(100.0),
                    ..default()
                })
                .with_children(|stripe| {
                    stripe.spawn((
                        ModalBackground,
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Percent(-100.0),
                            top: Val::Px(0.0),
                            bottom: Val::Px(0.0),
                            width: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
                    ));
                    stripe
                        .spawn((
                            ModalContent,
                            Node {
                                width: Val::Percent(100.0),
                                left: Val::Percent(100.0),
                                flex_direction: FlexDirection::Column,
                                align_items: AlignItems::Center,
                                padding: UiRect::vertical(Val::Px(28.0)),
                                row_gap: Val::Px(14.0),
                                ..default()
                            },
                        ))
                        .with_children(|content| {
                            content.spawn((
                                Text::new("Player Options"),
                                font.sized(52.0),
                                TextColor(TITLE_COLOR),
                                Node {
                                    margin: UiRect::bottom(Val::Px(24.0)),
                                    ..default()
                                },
                            ));
                            for index in 0..OptionRow::COUNT {
                                let (values, selected) =
                                    row_values(row(index), &settings, &config, &skins);
                                content
                                    .spawn(Node {
                                        column_gap: Val::Px(24.0),
                                        align_items: AlignItems::Center,
                                        ..default()
                                    })
                                    .with_children(|option| {
                                        spawn_option_row(option, &font, index, values, selected);
                                    });
                            }
                        });
                });
        });
}

fn spawn_option_row(
    option: &mut ChildSpawnerCommands,
    font: &GameFont,
    index: usize,
    values: Vec<String>,
    selected: usize,
) {
    let label: &str = row(index).into();
    option
        .spawn(Node {
            width: Val::Px(240.0),
            ..default()
        })
        .with_children(|cell| {
            cell.spawn((
                RowText(index),
                Text::new(label),
                font.sized(28.0),
                TextColor(INACTIVE_COLOR),
            ));
        });
    option
        .spawn((
            RowValues(index),
            Node {
                column_gap: Val::Px(22.0),
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .with_children(|list| spawn_values(list, font, index, values));
    option.spawn((
        Underline {
            row: index,
            target: selected,
            current: None,
        },
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            bottom: Val::Px(-2.0),
            width: Val::Px(0.0),
            height: Val::Px(4.0),
            border_radius: BorderRadius::all(Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(STEPFILE_TEXT),
    ));
}

fn spawn_values(list: &mut ChildSpawnerCommands, font: &GameFont, row: usize, values: Vec<String>) {
    for (index, value) in values.into_iter().enumerate() {
        list.spawn((
            ValueText { row, index },
            Text::new(value),
            font.sized(28.0),
            TextColor(INACTIVE_COLOR),
        ));
    }
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

/// The value lists depend on the settings (the modifier list follows the
/// speed type), so they are respawned whenever the settings change.
fn rebuild_value_lists(
    settings: Res<Settings>,
    config: Res<GameConfig>,
    skins: Res<NoteSkinLibrary>,
    font: Res<GameFont>,
    containers: Query<(Entity, &RowValues)>,
    mut underlines: Query<&mut Underline>,
    mut commands: Commands,
) {
    if !settings.is_changed() {
        return;
    }
    for (container, marker) in &containers {
        let (values, selected) = row_values(row(marker.0), &settings, &config, &skins);
        for mut underline in &mut underlines {
            if underline.row == marker.0 {
                underline.target = selected;
            }
        }
        commands.entity(container).despawn_related::<Children>();
        commands
            .entity(container)
            .with_children(|list| spawn_values(list, &font, marker.0, values));
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
    font: Res<GameFont>,
    text: Option<Single<&mut Text2d, With<OptionsSummary>>>,
    mut commands: Commands,
) {
    let Some(mut text) = text else {
        commands.spawn((
            DespawnOnExit(GameScene::FileSelect),
            OptionsSummary,
            Text2d::new(summary(&settings, &skins)),
            font.sized(22.0),
            TextColor(STATS_TEXT),
            Transform::from_xyz(-320.0, -150.0, 5.0),
        ));
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

#[derive(Component)]
struct ActiveRow(usize);

/// `t` runs 0..=1 through the open/close effect; `dir` is +1 while opening
/// and -1 while closing.
#[derive(Component)]
struct ModalTransition {
    t: f32,
    dir: f32,
}

const TRANSITION_SECONDS: f32 = 0.25;

#[derive(Component)]
struct ModalBackground;

#[derive(Component)]
struct ModalContent;

#[derive(Component)]
struct RowText(usize);

#[derive(Component)]
struct RowValues(usize);

#[derive(Component)]
struct ValueText {
    row: usize,
    index: usize,
}

/// One per option row; [`animate_underline`] tweens it to the selected value.
#[derive(Component)]
struct Underline {
    row: usize,
    target: usize,
    current: Option<Vec2>,
}

#[derive(Component)]
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
