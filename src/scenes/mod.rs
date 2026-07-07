pub mod file_player;
pub mod file_select;
pub mod keymap;
pub mod main_menu;
pub mod player_options;
pub mod score;
pub mod settings_menu;

use crate::core::menu::MenuPlugin;
use crate::core::scene_flow::SceneFlowPlugin;
use bevy::prelude::*;

/// Every scene in the game. Scene systems run under `in_state`, scene
/// entities carry `DespawnOnExit(GameScene::...)`.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GameScene {
    #[default]
    MainMenu,
    SettingsMenu,
    Keymap,
    FileSelect,
    PlayerOptions,
    FilePlayer,
    Score,
}

/// The scene fade driving [`GameScene`] transitions.
pub type SceneFade = crate::core::scene_flow::SceneFade<GameScene>;

/// Run condition: the current scene is fully faded in and accepting input.
pub fn scene_accepts_input(fade: Res<SceneFade>) -> bool {
    fade.accepts_input()
}

pub struct ScenesPlugin;

impl Plugin for ScenesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            SceneFlowPlugin::<GameScene>::default(),
            MenuPlugin::<GameScene>::default(),
            main_menu::MainMenuPlugin,
            settings_menu::SettingsMenuPlugin,
            keymap::KeymapScenePlugin,
            file_select::FileSelectPlugin,
            player_options::PlayerOptionsPlugin,
            file_player::FilePlayerPlugin,
            score::ScoreScenePlugin,
        ));
    }
}
