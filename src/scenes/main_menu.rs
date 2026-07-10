use crate::core::scene_flow::SpawnScoped;
use crate::prefabs::menu::{MenuPrefabOptions, MenuSelected, menu_prefab};
use crate::scenes::{GameScene, SceneFade, play_default_bgm, spawn_default_background};
use bevy::prelude::*;

pub(super) struct MainMenuPlugin;

impl Plugin for MainMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(GameScene::MainMenu),
            (enter, play_default_bgm, spawn_default_background),
        )
        .add_systems(
            Update,
            handle_selection.run_if(in_state(GameScene::MainMenu)),
        );
    }
}

fn enter(mut commands: Commands) {
    commands.spawn_scoped(
        GameScene::MainMenu,
        menu_prefab(MenuPrefabOptions {
            title: "Rhythm".to_string(),
            items: vec![
                "Start".to_string(),
                "Settings".to_string(),
                "Quit".to_string(),
            ],
        }),
    );
}

fn handle_selection(
    mut selected: MessageReader<MenuSelected>,
    mut fade: ResMut<SceneFade>,
    mut exit: MessageWriter<AppExit>,
) {
    for selection in selected.read() {
        match selection.index {
            0 => fade.begin(GameScene::ModeSelect),
            1 => fade.begin(GameScene::SettingsMenu),
            _ => {
                exit.write(AppExit::Success);
            }
        }
    }
}
