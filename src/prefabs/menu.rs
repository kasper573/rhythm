use crate::core::font::game_font;
use crate::core::input::{Actions, GameAction, NavPulse};
use crate::core::player::PlayerId;
use crate::core::scene_flow::SceneFade;
use crate::core::sfx::{PlaySfx, Sfx};
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
/// teardown); [`MenuPlugin`] drives every mounted menu: ¤Next¤/¤Previous¤
/// pulses move the highlight and P1's ¤Select¤ fires [`MenuSelected`] with
/// the active item's index. Custom layouts get the same driving by carrying
/// [`Menu`] and [`MenuItem`] themselves.
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

/// While set, menus ignore input — for owners that capture the keyboard
/// (the keymap scene's rebind prompt).
#[derive(Resource, Default)]
pub struct MenuInputLock(pub bool);

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
        app.init_resource::<MenuInputLock>()
            .add_message::<MenuSelected>()
            .add_systems(
                Update,
                (menu_navigate, menu_select, menu_highlight)
                    .chain()
                    .run_if(menus_active::<S>),
            );
    }
}

fn menus_active<S: FreelyMutableState>(lock: Res<MenuInputLock>, fade: Res<SceneFade<S>>) -> bool {
    !lock.0 && fade.accepts_input()
}

fn menu_navigate(
    mut pulses: MessageReader<NavPulse>,
    mut menus: Query<&mut Menu>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    for pulse in pulses.read() {
        let step_back = match pulse.action {
            GameAction::Previous => true,
            GameAction::Next => false,
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
    menus: Query<&Menu>,
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
