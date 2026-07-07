use crate::core::config::GameConfig;
use crate::core::font::GameFont;
use crate::core::input::{Actions, GameAction, NavPulse};
use crate::core::note_field::NoteSpeed;
use crate::core::note_skin::NoteSkinLibrary;
use crate::core::scene_flow::{GameScene, SceneFade, scene_accepts_input};
use crate::core::settings::Settings;
use crate::core::sfx::{PlaySfx, Sfx};
use bevy::prelude::*;

/// A matrix of the player's stepfile options: ¤Next¤/¤Previous¤ move between
/// rows, ¤Left¤/¤Right¤ swap the active row's value, ¤Cancel¤ returns to the
/// file select scene. Values live in the settings, so edits persist
/// immediately. Reached by holding ¤Select¤ on the wheel; the wheel row to
/// return to stays parked in [`FileSelectTarget`](crate::scenes::file_select::FileSelectTarget)
/// for the trip back.
pub struct PlayerOptionsPlugin;

impl Plugin for PlayerOptionsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameScene::PlayerOptions), enter)
            .add_systems(OnExit(GameScene::PlayerOptions), exit)
            .add_systems(
                Update,
                (handle_pulses, handle_cancel, refresh_rows).chain().run_if(
                    in_state(GameScene::PlayerOptions)
                        .and_then(scene_accepts_input)
                        .and_then(resource_exists::<ActiveRow>),
                ),
            );
    }
}

/// Rows of the options matrix, in display order.
const ROWS: [OptionRow; 3] = [
    OptionRow::SpeedType,
    OptionRow::SpeedModifier,
    OptionRow::NoteSkin,
];

#[derive(Debug, Clone, Copy, PartialEq)]
enum OptionRow {
    SpeedType,
    SpeedModifier,
    NoteSkin,
}

impl OptionRow {
    fn label(self) -> &'static str {
        match self {
            OptionRow::SpeedType => "Speed Type",
            OptionRow::SpeedModifier => "Speed Modifier",
            OptionRow::NoteSkin => "Note Skin",
        }
    }
}

#[derive(Resource)]
struct ActiveRow(usize);

#[derive(Component)]
struct RowText(usize);

const ACTIVE_COLOR: Color = Color::WHITE;
const INACTIVE_COLOR: Color = Color::srgb(0.45, 0.45, 0.55);

fn enter(mut commands: Commands, font: Res<GameFont>) {
    commands.insert_resource(ActiveRow(0));
    commands
        .spawn((
            DespawnOnExit(GameScene::PlayerOptions),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(14.0),
                ..default()
            },
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Player Options"),
                font.sized(52.0),
                TextColor(Color::srgb(0.95, 0.85, 0.4)),
                Node {
                    margin: UiRect::bottom(Val::Px(24.0)),
                    ..default()
                },
            ));
            for index in 0..ROWS.len() {
                parent.spawn((
                    RowText(index),
                    Text::new(""),
                    font.sized(28.0),
                    TextColor(INACTIVE_COLOR),
                ));
            }
        });
}

fn exit(mut commands: Commands) {
    commands.remove_resource::<ActiveRow>();
}

fn handle_pulses(
    mut pulses: MessageReader<NavPulse>,
    mut active: ResMut<ActiveRow>,
    mut settings: ResMut<Settings>,
    config: Res<GameConfig>,
    skins: Res<NoteSkinLibrary>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    for pulse in pulses.read() {
        match pulse.action {
            GameAction::Previous => {
                active.0 = (active.0 + ROWS.len() - 1) % ROWS.len();
                sfx.write(PlaySfx(Sfx::Navigate));
            }
            GameAction::Next => {
                active.0 = (active.0 + 1) % ROWS.len();
                sfx.write(PlaySfx(Sfx::Navigate));
            }
            GameAction::Left | GameAction::Right => {
                let delta = if pulse.action == GameAction::Left {
                    -1
                } else {
                    1
                };
                if change_value(ROWS[active.0], delta, &mut settings, &config, &skins) {
                    sfx.write(PlaySfx(Sfx::Navigate));
                }
            }
            _ => {}
        }
    }
}

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

fn handle_cancel(actions: Actions, mut fade: ResMut<SceneFade>, mut sfx: MessageWriter<PlaySfx>) {
    if actions.just_pressed(GameAction::Cancel) {
        sfx.write(PlaySfx(Sfx::Cancel));
        fade.begin(GameScene::FileSelect);
    }
}

fn refresh_rows(
    active: Res<ActiveRow>,
    settings: Res<Settings>,
    config: Res<GameConfig>,
    skins: Res<NoteSkinLibrary>,
    mut rows: Query<(&RowText, &mut Text, &mut TextColor)>,
) {
    if !active.is_changed() && !settings.is_changed() {
        return;
    }
    for (row, mut text, mut color) in &mut rows {
        let (values, selected) = row_values(ROWS[row.0], &settings, &config, &skins);
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
        text.0 = format!("{:<18} {}", ROWS[row.0].label(), values.join("   "));
        color.0 = if row.0 == active.0 {
            ACTIVE_COLOR
        } else {
            INACTIVE_COLOR
        };
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
                    .map(|value| format_value(*value))
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

/// Whole numbers without a fraction ("300"), fractions as-is ("0.25").
fn format_value(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}
