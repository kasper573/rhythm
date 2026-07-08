use super::{FileSelectMode, STEPFILE_TEXT};
use crate::core::config::GameConfig;
use crate::core::font::GameFont;
use crate::core::input::{Actions, GameAction, NavPulse};
use crate::core::menu::{ACTIVE_COLOR, INACTIVE_COLOR, TITLE_COLOR};
use crate::core::note_field::NoteSpeed;
use crate::core::note_skin::NoteSkinLibrary;
use crate::core::settings::Settings;
use crate::core::sfx::{PlaySfx, Sfx};
use bevy::prelude::*;
use strum::{EnumCount, EnumIter, IntoEnumIterator, IntoStaticStr};

/// One line summarizing the current options, e.g. `Dynamic 2x · DDREx Note`.
pub(super) fn summary(settings: &Settings, skins: &NoteSkinLibrary) -> String {
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

/// Edits the stepfile options in place: they live in the settings, so
/// changes persist immediately. An edge-to-edge stripe over the vertical
/// center of the file select, which stays mounted underneath. The
/// background and the content are siblings so the transition can slide
/// them in from opposite directions.
pub(super) fn enter(mut commands: Commands, font: Res<GameFont>) {
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
                                content
                                    .spawn(Node {
                                        column_gap: Val::Px(24.0),
                                        align_items: AlignItems::Center,
                                        ..default()
                                    })
                                    .with_children(|option| {
                                        let label: &str = row(index).into();
                                        option.spawn((
                                            RowText(index),
                                            Text::new(label),
                                            font.sized(28.0),
                                            TextColor(INACTIVE_COLOR),
                                            Node {
                                                width: Val::Px(240.0),
                                                ..default()
                                            },
                                        ));
                                        option.spawn((
                                            RowValues(index),
                                            Node {
                                                column_gap: Val::Px(22.0),
                                                align_items: AlignItems::Center,
                                                ..default()
                                            },
                                        ));
                                    });
                            }
                        });
                });
        });
}

pub(super) fn handle_pulses(
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

pub(super) fn handle_close(
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

#[allow(clippy::too_many_arguments)]
pub(super) fn refresh_rows(
    active: Single<Ref<ActiveRow>>,
    settings: Res<Settings>,
    config: Res<GameConfig>,
    skins: Res<NoteSkinLibrary>,
    font: Res<GameFont>,
    mut labels: Query<(&RowText, &mut TextColor)>,
    containers: Query<(Entity, &RowValues)>,
    mut commands: Commands,
) {
    if !active.is_changed() && !settings.is_changed() {
        return;
    }
    for (label, mut color) in &mut labels {
        color.0 = row_color(label.0 == active.0);
    }
    for (container, marker) in &containers {
        let color = row_color(marker.0 == active.0);
        let (values, selected) = row_values(row(marker.0), &settings, &config, &skins);
        commands.entity(container).despawn_related::<Children>();
        commands.entity(container).with_children(|list| {
            for (index, value) in values.into_iter().enumerate() {
                list.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(3.0),
                    ..default()
                })
                .with_children(|item| {
                    item.spawn((Text::new(value), font.sized(28.0), TextColor(color)));
                    item.spawn((
                        Underline,
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Px(4.0),
                            border_radius: BorderRadius::all(Val::Px(2.0)),
                            ..default()
                        },
                        BackgroundColor(STEPFILE_TEXT),
                        if index == selected {
                            Visibility::Inherited
                        } else {
                            Visibility::Hidden
                        },
                    ));
                });
            }
        });
    }
}

fn row_color(active: bool) -> Color {
    if active { ACTIVE_COLOR } else { INACTIVE_COLOR }
}

/// The background slides in from the left and the content from the right,
/// both fading in; closing plays the same effect in reverse and only then
/// leaves the modal state.
#[allow(clippy::type_complexity)]
pub(super) fn animate_transition(
    time: Res<Time>,
    mut mode: ResMut<NextState<FileSelectMode>>,
    mut modal: Single<&mut ModalTransition>,
    mut background: Single<(&mut Node, &mut BackgroundColor), With<ModalBackground>>,
    mut content: Single<&mut Node, (With<ModalContent>, Without<ModalBackground>)>,
    mut texts: Query<&mut TextColor, With<Node>>,
    mut underlines: Query<&mut BackgroundColor, (With<Underline>, Without<ModalBackground>)>,
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
pub(super) struct ActiveRow(usize);

/// `t` runs 0..=1 through the open/close effect; `dir` is +1 while opening
/// and -1 while closing.
#[derive(Component)]
pub(super) struct ModalTransition {
    t: f32,
    dir: f32,
}

const TRANSITION_SECONDS: f32 = 0.25;

#[derive(Component)]
pub(super) struct ModalBackground;

#[derive(Component)]
pub(super) struct ModalContent;

#[derive(Component)]
pub(super) struct RowText(usize);

#[derive(Component)]
pub(super) struct RowValues(usize);

#[derive(Component)]
pub(super) struct Underline;

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
