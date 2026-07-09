use super::{FileSelectFocus, STATS_TEXT};
use crate::core::at;
use crate::core::config::GameConfig;
use crate::core::font::game_font;
use crate::core::input::{Actions, GameAction, NavPulse, StepDirection};
use crate::core::menu::{ACTIVE_COLOR, INACTIVE_COLOR, TITLE_COLOR};
use crate::core::note_field::NoteSpeed;
use crate::core::note_skin::NoteSkinLibrary;
use crate::core::player::{PlayMode, PlayerId};
use crate::core::scene_flow::SpawnScoped;
use crate::core::settings::{PlayerOptions, PlayerSettings};
use crate::core::sfx::{PlaySfx, Sfx};
use crate::scenes::GameScene;
use bevy::ecs::query::QueryFilter;
use bevy::prelude::*;
use strum::{EnumCount, EnumIter, IntoEnumIterator, IntoStaticStr};

/// The player options modal: edits each active player's options in place
/// (they live in the player settings, so changes persist immediately) as
/// an edge-to-edge stripe over the vertical center of the file select,
/// which stays mounted underneath. One options panel per active player —
/// P1 to the left, P2 to the right — each driven by its player's own pad.
/// Also keeps the options summary next to the wheel current.
pub(super) fn plugin(app: &mut App) {
    app.add_systems(OnEnter(FileSelectFocus::PlayerOptions), enter)
        .add_systems(
            Update,
            refresh_summary.run_if(in_state(GameScene::FileSelect)),
        )
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
    let tagged = mode.players().len() > 1;
    let panels: Vec<_> = mode
        .players()
        .iter()
        .map(|player| panel_scene(*player, tagged, &settings[*player], &config, &skins))
        .collect();
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
                            row_gap: px(18),
                        }
                        Children [
                            (
                                ModalText
                                game_font(52.0)
                                Text("Player Options")
                                TextColor({TITLE_COLOR})
                                Node { margin: {UiRect::bottom(Val::Px(16.0))} }
                            ),
                            (
                                Node {
                                    column_gap: px(110),
                                    align_items: AlignItems::FlexStart,
                                }
                                Children [ {panels} ]
                            ),
                        ]
                    ),
                ]
            )]
        },
    );
}

/// One player's column of option rows, each showing only its selected
/// value; ¤Up¤/¤Down¤ move between rows, ¤Left¤/¤Right¤ change the value
/// in place.
fn panel_scene(
    player: PlayerId,
    tagged: bool,
    options: &PlayerOptions,
    config: &GameConfig,
    skins: &NoteSkinLibrary,
) -> impl Scene {
    let header: Vec<_> = tagged
        .then(|| {
            let tag = player.label().to_string();
            bsn! {
                ModalText
                game_font(34.0)
                Text({tag})
                TextColor({TITLE_COLOR})
                Node { margin: {UiRect::bottom(Val::Px(8.0))} }
            }
        })
        .into_iter()
        .collect();
    let rows: Vec<_> = OptionRow::iter()
        .enumerate()
        .map(|(index, row)| {
            let label: &str = row.into();
            let value = row_value(row, options, config, skins);
            bsn! {
                Node { column_gap: px(24), align_items: AlignItems::Center }
                Children [
                    (
                        Node { width: px(220) }
                        Children [(
                            RowText { player: {player}, row: index }
                            ModalText
                            game_font(28.0)
                            Text({label.to_string()})
                            TextColor({INACTIVE_COLOR})
                        )]
                    ),
                    (
                        ValueText { player: {player}, row: index }
                        ModalText
                        game_font(28.0)
                        Text({value})
                        TextColor({INACTIVE_COLOR})
                    ),
                ]
            }
        })
        .collect();
    bsn! {
        OptionsPanel { player: {player}, active_row: 0 }
        Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexStart,
            row_gap: px(14),
        }
        Children [
            {header},
            {rows},
        ]
    }
}

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
    let active = |player: PlayerId, row: usize| {
        panels
            .iter()
            .any(|panel| panel.player == player && panel.active_row == row)
    };
    for (label, mut color) in &mut labels {
        let wanted = row_color(active(label.player, label.row));
        if color.0 != wanted {
            color.0 = wanted;
        }
    }
    for (value, mut color) in &mut values {
        let wanted = row_color(active(value.player, value.row));
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

/// Upserts the options summary shown next to the wheel — one line per
/// active player, tagged when both play, e.g. `P1  Dynamic 2x · DDREx Note`.
fn refresh_summary(
    mode: Res<PlayMode>,
    settings: Res<PlayerSettings>,
    skins: Res<NoteSkinLibrary>,
    text: Option<Single<&mut Text2d, With<OptionsSummary>>>,
    mut commands: Commands,
) {
    let Some(mut text) = text else {
        let lines = summary(*mode, &settings, &skins);
        commands.spawn_scoped(
            GameScene::FileSelect,
            bsn! {
                OptionsSummary
                game_font(22.0)
                Text2d({lines})
                TextColor({STATS_TEXT})
                at(-320.0, -150.0, 5.0)
            },
        );
        return;
    };
    if settings.is_changed() {
        text.0 = summary(*mode, &settings, &skins);
    }
}

fn summary(mode: PlayMode, settings: &PlayerSettings, skins: &NoteSkinLibrary) -> String {
    let tagged = mode.players().len() > 1;
    mode.players()
        .iter()
        .map(|player| {
            let options = &settings[*player];
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
            if tagged {
                format!("{}  {speed} · {skin}", player.label())
            } else {
                format!("{speed} · {skin}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
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
    player: PlayerId,
    row: usize,
}

#[derive(Component, Clone, FromTemplate)]
struct ValueText {
    player: PlayerId,
    row: usize,
}

#[derive(Component, Default, Clone)]
struct OptionsSummary;

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
