pub mod audio_settings;
pub mod keymap;
pub mod main_menu;
pub mod mode_select;
pub mod play;
pub mod score;
pub mod settings_menu;
pub mod wheel;

use crate::core::library::library;
use crate::core::stepfile::MusicPlayer;
use crate::core::units::Seconds;
use crate::game::Game;
use crate::nodes::media_cover::{MediaCover, MediaCoverOptions, MediaPace};
use godot::classes::{Control, Node};
use godot::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GameScene {
    #[default]
    MainMenu,
    ModeSelect,
    SettingsMenu,
    Keymap,
    AudioSettings,
    Wheel,
    Play,
    Score,
}

/// The scene node the router swaps in for each [`GameScene`]. Runs inside
/// the router's own frame, so route params flow through `game` directly —
/// the singleton is mutably bound and cannot be re-entered here.
pub fn instantiate_scene(scene: GameScene, game: &mut Game) -> Gd<Node> {
    match scene {
        GameScene::MainMenu => main_menu::MainMenuScene::instantiate().upcast(),
        GameScene::ModeSelect => mode_select::ModeSelectScene::instantiate().upcast(),
        GameScene::SettingsMenu => settings_menu::SettingsMenuScene::instantiate().upcast(),
        GameScene::Keymap => keymap::KeymapScene::instantiate().upcast(),
        GameScene::AudioSettings => audio_settings::AudioSettingsScene::instantiate().upcast(),
        GameScene::Wheel => wheel::WheelScene::instantiate(game).upcast(),
        GameScene::Play => play::PlayScene::instantiate(game).upcast(),
        GameScene::Score => score::ScoreScene::instantiate(game).upcast(),
    }
}

pub fn change_scene(to: GameScene) {
    Game::singleton().bind_mut().change_scene(to);
}

pub fn scene_accepts_input() -> bool {
    Game::singleton().bind().accepts_input()
}

/// Scenes without music of their own start the default BGM on enter; the
/// player keeps it running across such scenes uninterrupted.
fn play_default_bgm() {
    let bgm = library().default_bgm.bgm();
    MusicPlayer::singleton().bind_mut().play(bgm);
}

/// The default BGM's background — its looping video, dimmed — behind the
/// entered scene's UI. Torn down with the scene; the scene fade masks the
/// remount.
fn spawn_default_background(scene: &mut Control) {
    let Some(path) = library().default_bgm.background_path() else {
        return;
    };
    let cover = MediaCover::instantiate(MediaCoverOptions {
        path,
        // Dimmed so the scene's text stays readable in front.
        color: Color::from_rgb(0.5, 0.5, 0.5),
        z: -100,
        start: Seconds::ZERO,
        looping: true,
        pace: MediaPace::Wall,
    });
    if let Some(cover) = cover {
        scene.add_child(&cover);
    }
}
