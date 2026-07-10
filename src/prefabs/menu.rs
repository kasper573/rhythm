use crate::core::font::game_font;
use crate::core::input::{Actions, GameAction, StepDirection};
use crate::core::player::PlayerId;
use crate::core::scene_flow::SceneFade;
use crate::core::sfx::{PlaySfx, Sfx};
use crate::core::units::Seconds;
use bevy::prelude::*;
use bevy::state::state::FreelyMutableState;
use std::marker::PhantomData;

pub const TITLE_COLOR: Color = Color::srgb(0.95, 0.85, 0.4);
pub const ACTIVE_COLOR: Color = Color::WHITE;
pub const INACTIVE_COLOR: Color = Color::srgb(0.45, 0.45, 0.55);

pub struct MenuPrefabOptions {
    pub title: String,
    pub items: Vec<String>,
}

/// A full-screen titled menu. The caller owns the spawn (and thus scope and
/// teardown); [`MenuPlugin`] drives every mounted menu: P1's ¤Up¤/¤Down¤
/// step pulses move the highlight and their ¤Select¤ fires [`MenuSelected`]
/// with the active item's index. Custom layouts get the same driving by
/// carrying [`Menu`] and [`MenuItem`] themselves.
pub fn menu_prefab(opt: MenuPrefabOptions) -> impl Scene {
    let title = opt.title;
    let len = opt.items.len();
    let entries: Vec<_> = opt
        .items
        .into_iter()
        .enumerate()
        .map(|(index, label)| {
            let color = if index == 0 {
                ACTIVE_COLOR
            } else {
                INACTIVE_COLOR
            };
            bsn! {
                MenuItem(index)
                game_font(34.0)
                Text({label})
                TextColor({color})
            }
        })
        .collect();
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: px(16),
        }
        Children [
            (
                game_font(52.0)
                Text({title})
                TextColor({TITLE_COLOR})
                Node { margin: {UiRect::bottom(Val::Px(32.0))} }
            ),
            (
                Menu { active: 0, len: len }
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(12),
                }
                Children [ {entries} ]
            ),
        ]
    }
}

#[derive(Component, Clone, FromTemplate)]
pub struct Menu {
    pub active: usize,
    pub len: usize,
}

#[derive(Component, Clone, FromTemplate)]
pub struct MenuItem(pub usize);

#[derive(Message)]
pub struct MenuSelected {
    pub index: usize,
}

/// Marks a menu whose owner drives `Menu::active` and selection itself; the
/// plugin only runs its highlight. For owners that must not depend on the
/// rebindable keymap — a keymap editor has to stay operable however broken
/// the stored bindings are.
#[derive(Component, Default, Clone)]
pub struct OwnerDrivenMenu;

/// A press of a step action, re-fired while held so lists scroll
/// comfortably — the navigation vocabulary menus and menu-like scenes
/// (the wheel, options panels) consume instead of raw key state.
#[derive(Message)]
pub struct NavPulse {
    pub action: GameAction,
}

/// The pulse emitter's slot in `PreUpdate`: synthetic key state (the
/// bench scenarios) must land after bevy's input update and before this
/// set to register on the frame it was written.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NavPulseSystems;

/// Menus pause while the scene fade for `S` is running, so the plugin is
/// generic over the app's scene state.
pub struct MenuPlugin<S>(PhantomData<S>);

impl<S> Default for MenuPlugin<S> {
    fn default() -> Self {
        MenuPlugin(PhantomData)
    }
}

impl<S: FreelyMutableState> Plugin for MenuPlugin<S> {
    fn build(&self, app: &mut App) {
        app.add_message::<NavPulse>()
            .add_message::<MenuSelected>()
            .add_systems(
                PreUpdate,
                emit_nav_pulses
                    .in_set(NavPulseSystems)
                    .after(bevy::input::InputSystems),
            )
            .add_systems(
                Update,
                (menu_navigate, menu_select, menu_highlight)
                    .chain()
                    .run_if(menus_active::<S>),
            );
    }
}

fn menus_active<S: FreelyMutableState>(fade: Res<SceneFade<S>>) -> bool {
    fade.accepts_input()
}

const REPEAT_DELAY: Seconds = Seconds(0.4);
const REPEAT_INTERVAL: Seconds = Seconds(0.09);

/// Every action lists and panels scroll by: each player's step panel.
const PULSE_ACTIONS: [GameAction; 8] = [
    GameAction::P1Left,
    GameAction::P1Down,
    GameAction::P1Up,
    GameAction::P1Right,
    GameAction::P2Left,
    GameAction::P2Down,
    GameAction::P2Up,
    GameAction::P2Right,
];

fn emit_nav_pulses(
    actions: Actions,
    time: Res<Time>,
    mut held: Local<[Seconds; PULSE_ACTIONS.len()]>,
    mut pulses: MessageWriter<NavPulse>,
) {
    for (slot, action) in PULSE_ACTIONS.into_iter().enumerate() {
        if actions.just_pressed(action) {
            held[slot] = Seconds::ZERO;
            pulses.write(NavPulse { action });
        } else if actions.pressed(action) {
            let before = held[slot];
            held[slot] += Seconds(time.delta_secs_f64());
            let repeats_before = ((before - REPEAT_DELAY) / REPEAT_INTERVAL).floor();
            let repeats_after = ((held[slot] - REPEAT_DELAY) / REPEAT_INTERVAL).floor();
            if held[slot] >= REPEAT_DELAY && repeats_after > repeats_before {
                pulses.write(NavPulse { action });
            }
        } else {
            held[slot] = Seconds::ZERO;
        }
    }
}

/// Menus are P1's space: their ¤Up¤/¤Down¤ steps move the highlight.
fn menu_navigate(
    mut pulses: MessageReader<NavPulse>,
    mut menus: Query<&mut Menu, Without<OwnerDrivenMenu>>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    for pulse in pulses.read() {
        let Some((PlayerId::P1, direction)) = pulse.action.as_step() else {
            continue;
        };
        let step_back = match direction {
            StepDirection::Up => true,
            StepDirection::Down => false,
            _ => continue,
        };
        for mut menu in &mut menus {
            if menu.len == 0 {
                continue;
            }
            menu.active = if step_back {
                (menu.active + menu.len - 1) % menu.len
            } else {
                (menu.active + 1) % menu.len
            };
            sfx.write(PlaySfx(Sfx::Navigate));
        }
    }
}

/// Menus are P1's space: only their ¤Select¤ activates an item.
fn menu_select(
    actions: Actions,
    menus: Query<&Menu, Without<OwnerDrivenMenu>>,
    mut selected: MessageWriter<MenuSelected>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    if !actions.just_pressed(GameAction::select(PlayerId::P1)) {
        return;
    }
    for menu in &menus {
        if menu.len == 0 {
            continue;
        }
        selected.write(MenuSelected { index: menu.active });
        sfx.write(PlaySfx(Sfx::Select));
    }
}

fn menu_highlight(
    menus: Query<(&Menu, &Children), Changed<Menu>>,
    mut items: Query<(&MenuItem, &mut TextColor)>,
) {
    for (menu, children) in &menus {
        for child in children.iter() {
            if let Ok((item, mut color)) = items.get_mut(child) {
                color.0 = if item.0 == menu.active {
                    ACTIVE_COLOR
                } else {
                    INACTIVE_COLOR
                };
            }
        }
    }
}
