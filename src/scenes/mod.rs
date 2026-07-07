pub mod file_player;
pub mod file_select;
pub mod keymap;
pub mod main_menu;
pub mod player_options;
pub mod score;
pub mod settings_menu;

use bevy::prelude::*;

pub struct ScenesPlugin;

impl Plugin for ScenesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
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
