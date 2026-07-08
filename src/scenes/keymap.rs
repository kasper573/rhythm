use crate::core::config::GameConfig;
use crate::core::font::game_font;
use crate::core::input::{Actions, GameAction};
use crate::core::menu::{INACTIVE_COLOR, Menu, MenuInputLock, MenuItem, MenuSelected, TITLE_COLOR};
use crate::core::scene_flow::SpawnScoped;
use crate::core::settings::Settings;
use crate::core::sfx::{PlaySfx, Sfx};
use crate::scenes::{
    GameScene, SceneFade, play_default_bgm, scene_accepts_input, spawn_default_background,
};
use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use strum::{EnumCount, IntoEnumIterator};

pub struct KeymapScenePlugin;

impl Plugin for KeymapScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(GameScene::Keymap),
            (enter, play_default_bgm, spawn_default_background),
        )
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

#[derive(Component, Default, Clone)]
struct PromptLabel;

fn enter(mut commands: Commands, settings: Res<Settings>, config: Res<GameConfig>) {
    commands.init_resource::<Prompt>();
    let rows: Vec<_> = GameAction::iter()
        .enumerate()
        .map(|(index, action)| {
            let label = row_label(action, &settings, &config);
            bsn! {
                MenuItem(index)
                game_font(26.0)
                Text({label})
                TextColor({INACTIVE_COLOR})
            }
        })
        .collect();
    commands.spawn_scoped(
        GameScene::Keymap,
        bsn! {
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: px(10),
            }
            Children [
                (
                    game_font(52.0)
                    Text("Keymap")
                    TextColor({TITLE_COLOR})
                    Node { margin: {UiRect::bottom(Val::Px(24.0))} }
                ),
                (
                    Menu { active: 0, len: {GameAction::COUNT} }
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(6),
                    }
                    Children [ {rows} ]
                ),
                (
                    PromptLabel
                    game_font(26.0)
                    Text("")
                    TextColor(Color::srgb(0.4, 0.9, 0.6))
                    Node { margin: {UiRect::top(Val::Px(24.0))} }
                ),
            ]
        },
    );
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
        prompt.action = GameAction::iter().nth(selection.index);
        prompt.just_opened = true;
        lock.0 = true;
    }
}

fn capture_prompt_key(
    mut prompt: ResMut<Prompt>,
    mut keyboard: MessageReader<KeyboardInput>,
    mut settings: ResMut<Settings>,
    config: Res<GameConfig>,
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
        if event.key_code == settings.keymap.key(GameAction::Cancel, &config) {
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
    config: Res<GameConfig>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    if prompt.action.is_some()
        || !settings
            .keymap
            .just_pressed(&keys, GameAction::Reset, &config)
    {
        return;
    }
    for menu in &menus {
        let Some(action) = GameAction::iter().nth(menu.active) else {
            continue;
        };
        settings.keymap.reset(action);
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
    config: Res<GameConfig>,
    prompt: Res<Prompt>,
    mut rows: Query<(&MenuItem, &mut Text), Without<PromptLabel>>,
    mut prompt_label: Single<&mut Text, With<PromptLabel>>,
) {
    if settings.is_changed() {
        for (item, mut text) in &mut rows {
            let Some(action) = GameAction::iter().nth(item.0) else {
                continue;
            };
            text.0 = row_label(action, &settings, &config);
        }
    }
    if prompt.is_changed() {
        prompt_label.0 = match prompt.action {
            Some(action) => format!("Press a key for \"{}\" (Cancel aborts)", action.label()),
            None => String::new(),
        };
    }
}

fn row_label(action: GameAction, settings: &Settings, config: &GameConfig) -> String {
    format!(
        "{:<30} {:?}",
        action.label(),
        settings.keymap.key(action, config)
    )
}
