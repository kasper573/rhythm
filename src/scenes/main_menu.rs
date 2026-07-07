use crate::core::font::GameFont;
use crate::core::menu::{MenuSelected, spawn_menu};
use crate::core::scene_flow::{GameScene, SceneFade};
use bevy::prelude::*;

pub struct MainMenuPlugin;

impl Plugin for MainMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameScene::MainMenu), enter)
            .add_systems(
                Update,
                handle_selection.run_if(in_state(GameScene::MainMenu)),
            );
    }
}

fn enter(mut commands: Commands, font: Res<GameFont>) {
    spawn_menu(
        &mut commands,
        &font,
        GameScene::MainMenu,
        "Rhythm",
        &["File select", "Settings", "Quit"],
    );
}

fn handle_selection(
    mut selected: MessageReader<MenuSelected>,
    mut fade: ResMut<SceneFade>,
    mut exit: MessageWriter<AppExit>,
) {
    for selection in selected.read() {
        match selection.index {
            0 => fade.begin(GameScene::FileSelect),
            1 => fade.begin(GameScene::SettingsMenu),
            _ => {
                exit.write(AppExit::Success);
            }
        }
    }
}
