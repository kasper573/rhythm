use crate::core::font::GameFont;
use crate::core::input::{Actions, GameAction};
use crate::core::menu::{Menu, MenuInputLock, MenuItem, MenuSelected};
use crate::core::settings::Settings;
use crate::core::sfx::{PlaySfx, Sfx};
use crate::scenes::{GameScene, SceneFade, scene_accepts_input};
use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;

pub struct KeymapScenePlugin;

impl Plugin for KeymapScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameScene::Keymap), enter)
            .add_systems(OnExit(GameScene::Keymap), exit)
            .add_systems(
                Update,
                (
                    open_prompt,
                    capture_prompt_key,
                    reset_active_binding,
                    handle_cancel,
                    refresh_rows,
                )
                    .chain()
                    .run_if(in_state(GameScene::Keymap).and_then(scene_accepts_input)),
            );
    }
}

/// The rebind prompt: which action we are listening for, and whether the
/// prompt was opened this frame (whose key events must be ignored so the
/// ¤Select¤ press that opened it doesn't bind itself).
#[derive(Resource, Default)]
struct Prompt {
    action: Option<GameAction>,
    just_opened: bool,
}

#[derive(Component)]
struct PromptLabel;

fn enter(mut commands: Commands, settings: Res<Settings>, font: Res<GameFont>) {
    commands.init_resource::<Prompt>();
    commands
        .spawn((
            DespawnOnExit(GameScene::Keymap),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(10.0),
                ..default()
            },
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Keymap"),
                font.sized(52.0),
                TextColor(Color::srgb(0.95, 0.85, 0.4)),
                Node {
                    margin: UiRect::bottom(Val::Px(24.0)),
                    ..default()
                },
            ));
            parent
                .spawn((
                    Menu {
                        active: 0,
                        len: GameAction::ALL.len(),
                    },
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(6.0),
                        ..default()
                    },
                ))
                .with_children(|list| {
                    for (index, action) in GameAction::ALL.into_iter().enumerate() {
                        list.spawn((
                            MenuItem(index),
                            Text::new(row_label(action, &settings)),
                            font.sized(26.0),
                            TextColor(Color::srgb(0.45, 0.45, 0.55)),
                        ));
                    }
                });
            parent.spawn((
                PromptLabel,
                Text::new(""),
                font.sized(26.0),
                TextColor(Color::srgb(0.4, 0.9, 0.6)),
                Node {
                    margin: UiRect::top(Val::Px(24.0)),
                    ..default()
                },
            ));
        });
}

fn exit(mut commands: Commands, mut lock: ResMut<MenuInputLock>) {
    commands.remove_resource::<Prompt>();
    lock.0 = false;
}

fn open_prompt(
    mut selected: MessageReader<MenuSelected>,
    mut prompt: ResMut<Prompt>,
    mut lock: ResMut<MenuInputLock>,
) {
    for selection in selected.read() {
        prompt.action = Some(GameAction::ALL[selection.index]);
        prompt.just_opened = true;
        lock.0 = true;
    }
}

fn capture_prompt_key(
    mut prompt: ResMut<Prompt>,
    mut keyboard: MessageReader<KeyboardInput>,
    mut settings: ResMut<Settings>,
    mut lock: ResMut<MenuInputLock>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    let Some(action) = prompt.action else {
        keyboard.clear();
        return;
    };
    if prompt.just_opened {
        prompt.just_opened = false;
        keyboard.clear();
        return;
    }
    for event in keyboard.read() {
        if event.state != ButtonState::Pressed || event.repeat {
            continue;
        }
        if event.key_code == settings.keymap.key(GameAction::Cancel) {
            sfx.write(PlaySfx(Sfx::Cancel));
        } else {
            settings.keymap.set(action, event.key_code);
            sfx.write(PlaySfx(Sfx::Select));
        }
        prompt.action = None;
        lock.0 = false;
        break;
    }
}

fn reset_active_binding(
    keys: Res<ButtonInput<KeyCode>>,
    prompt: Res<Prompt>,
    menus: Query<&Menu>,
    mut settings: ResMut<Settings>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    if prompt.action.is_some() || !settings.keymap.just_pressed(&keys, GameAction::Reset) {
        return;
    }
    for menu in &menus {
        settings.keymap.reset(GameAction::ALL[menu.active]);
        sfx.write(PlaySfx(Sfx::Select));
    }
}

fn handle_cancel(
    actions: Actions,
    prompt: Res<Prompt>,
    mut fade: ResMut<SceneFade>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    if prompt.action.is_none() && actions.just_pressed(GameAction::Cancel) {
        sfx.write(PlaySfx(Sfx::Cancel));
        fade.begin(GameScene::SettingsMenu);
    }
}

fn refresh_rows(
    settings: Res<Settings>,
    prompt: Res<Prompt>,
    mut rows: Query<(&MenuItem, &mut Text), Without<PromptLabel>>,
    mut prompt_label: Single<&mut Text, With<PromptLabel>>,
) {
    if settings.is_changed() {
        for (item, mut text) in &mut rows {
            text.0 = row_label(GameAction::ALL[item.0], &settings);
        }
    }
    if prompt.is_changed() {
        prompt_label.0 = match prompt.action {
            Some(action) => format!("Press a key for \"{}\" (Cancel aborts)", action.label()),
            None => String::new(),
        };
    }
}

fn row_label(action: GameAction, settings: &Settings) -> String {
    format!("{:<30} {:?}", action.label(), settings.keymap.key(action))
}
