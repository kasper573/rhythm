pub mod file_player;
pub mod file_select;
pub mod keymap;
pub mod main_menu;
pub mod score;
pub mod settings_menu;

use crate::core::menu::MenuPlugin;
use crate::core::scene_flow::SceneFlowPlugin;
use bevy::prelude::*;

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GameScene {
    #[default]
    MainMenu,
    SettingsMenu,
    Keymap,
    FileSelect,
    FilePlayer,
    Score,
}

pub type SceneFade = crate::core::scene_flow::SceneFade<GameScene>;

/// Scenes without music of their own start the default BGM on enter; the
/// player keeps it running across such scenes uninterrupted.
pub fn play_default_bgm(
    library: Res<crate::core::library::StepfileLibrary>,
    mut music: ResMut<crate::core::stepfile::MusicPlayer>,
    mut commands: Commands,
) {
    music.play(&mut commands, library.default_bgm.bgm());
}

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
            file_player::FilePlayerPlugin,
            score::ScoreScenePlugin,
        ));
    }
}
