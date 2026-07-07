use crate::core::font::GameFont;
use crate::core::input::{Actions, GameAction, NavPulse};
use crate::core::scene_flow::SceneFade;
use crate::core::sfx::{PlaySfx, Sfx};
use bevy::prelude::*;
use bevy::state::state::FreelyMutableState;
use std::marker::PhantomData;

pub const TITLE_COLOR: Color = Color::srgb(0.95, 0.85, 0.4);
pub const ACTIVE_COLOR: Color = Color::WHITE;
pub const INACTIVE_COLOR: Color = Color::srgb(0.45, 0.45, 0.55);

#[derive(Component)]
pub struct Menu {
    pub active: usize,
    pub len: usize,
}

#[derive(Component)]
pub struct MenuItem(pub usize);

#[derive(Message)]
pub struct MenuSelected {
    pub index: usize,
}

#[derive(Resource, Default)]
pub struct MenuInputLock(pub bool);

pub fn spawn_menu<S: States>(
    commands: &mut Commands,
    font: &GameFont,
    scene: S,
    title: &str,
    items: &[&str],
) {
    commands
        .spawn((
            DespawnOnExit(scene),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(16.0),
                ..default()
            },
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(title),
                font.sized(52.0),
                TextColor(TITLE_COLOR),
                Node {
                    margin: UiRect::bottom(Val::Px(32.0)),
                    ..default()
                },
            ));
            parent
                .spawn((
                    Menu {
                        active: 0,
                        len: items.len(),
                    },
                    Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(12.0),
                        ..default()
                    },
                ))
                .with_children(|list| {
                    for (index, item) in items.iter().enumerate() {
                        list.spawn((
                            MenuItem(index),
                            Text::new(*item),
                            font.sized(34.0),
                            TextColor(if index == 0 {
                                ACTIVE_COLOR
                            } else {
                                INACTIVE_COLOR
                            }),
                        ));
                    }
                });
        });
}

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

fn menu_select(
    actions: Actions,
    menus: Query<&Menu>,
    mut selected: MessageWriter<MenuSelected>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    if !actions.just_pressed(GameAction::Select) {
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
