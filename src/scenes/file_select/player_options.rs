use super::FileSelectMode;
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
/// center of the file select, which stays mounted underneath.
pub(super) fn enter(mut commands: Commands, font: Res<GameFont>) {
    commands
        .spawn((
            DespawnOnExit(FileSelectMode::PlayerOptions),
            ActiveRow(0),
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
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        padding: UiRect::vertical(Val::Px(28.0)),
                        row_gap: Val::Px(14.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.0, 0.0, 0.0)),
                ))
                .with_children(|stripe| {
                    stripe.spawn((
                        Text::new("Player Options"),
                        font.sized(52.0),
                        TextColor(TITLE_COLOR),
                        Node {
                            margin: UiRect::bottom(Val::Px(24.0)),
                            ..default()
                        },
                    ));
                    for index in 0..OptionRow::COUNT {
                        stripe.spawn((
                            RowText(index),
                            Text::new(""),
                            font.sized(28.0),
                            TextColor(INACTIVE_COLOR),
                        ));
                    }
                });
        });
}

pub(super) fn handle_pulses(
    mut pulses: MessageReader<NavPulse>,
    mut active: Single<&mut ActiveRow>,
    mut settings: ResMut<Settings>,
    config: Res<GameConfig>,
    skins: Res<NoteSkinLibrary>,
    mut sfx: MessageWriter<PlaySfx>,
) {
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

pub(super) fn handle_cancel(
    actions: Actions,
    mut mode: ResMut<NextState<FileSelectMode>>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    if actions.just_pressed(GameAction::Cancel) {
        sfx.write(PlaySfx(Sfx::Cancel));
        mode.set(FileSelectMode::Browse);
    }
}

pub(super) fn refresh_rows(
    active: Single<Ref<ActiveRow>>,
    settings: Res<Settings>,
    config: Res<GameConfig>,
    skins: Res<NoteSkinLibrary>,
    mut rows: Query<(&RowText, &mut Text, &mut TextColor)>,
) {
    if !active.is_changed() && !settings.is_changed() {
        return;
    }
    for (row_text, mut text, mut color) in &mut rows {
        let (values, selected) = row_values(row(row_text.0), &settings, &config, &skins);
        let values: Vec<String> = values
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                if index == selected {
                    format!("[{value}]")
                } else {
                    value
                }
            })
            .collect();
        let label: &str = row(row_text.0).into();
        text.0 = format!("{label:<18} {}", values.join("   "));
        color.0 = if row_text.0 == active.0 {
            ACTIVE_COLOR
        } else {
            INACTIVE_COLOR
        };
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

#[derive(Component)]
pub(super) struct RowText(usize);

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
