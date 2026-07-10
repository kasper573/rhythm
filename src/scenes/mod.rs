pub mod audio_settings;
pub mod keymap;
pub mod main_menu;
pub mod mode_select;
pub mod play;
pub mod score;
pub mod settings_menu;
pub mod wheel;

use crate::core::library::StepfileLibrary;
use crate::core::scene_flow::SceneFlowPlugin;
use crate::core::stepfile::MusicPlayer;
use crate::core::units::Seconds;
use crate::prefabs::media_cover::{MediaCoverPrefabOptions, MediaPace, media_cover_prefab};
use crate::prefabs::menu::MenuPlugin;
use bevy::prelude::*;

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
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

pub type SceneFade = crate::core::scene_flow::SceneFade<GameScene>;

/// The default BGM's background — its looping video, dimmed — behind the
/// entered scene's UI. Registered on the `OnEnter` of scenes that want
/// it, torn down with the scene; the scene fade masks the remount.
fn spawn_default_background(
    mut commands: Commands,
    library: Res<StepfileLibrary>,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    time: Res<Time>,
    scene: Res<State<GameScene>>,
) {
    let Some(path) = library.default_bgm.background_path() else {
        return;
    };
    let cover = media_cover_prefab(
        MediaCoverPrefabOptions {
            path,
            // Dimmed so the scene's text stays readable in front.
            color: Color::srgb(0.5, 0.5, 0.5),
            z: -10.0,
            start: Seconds(time.elapsed_secs_f64()),
            looping: true,
            pace: MediaPace::Wall,
        },
        &mut commands,
        &asset_server,
        &mut images,
    );
    if let Some(cover) = cover {
        commands.entity(cover).insert(DespawnOnExit(*scene.get()));
    }
}

/// Scenes without music of their own start the default BGM on enter; the
/// player keeps it running across such scenes uninterrupted.
fn play_default_bgm(library: Res<StepfileLibrary>, mut music: ResMut<MusicPlayer>) {
    music.play(library.default_bgm.bgm());
}

fn scene_accepts_input(fade: Res<SceneFade>) -> bool {
    fade.accepts_input()
}

pub(crate) struct ScenesPlugin;

impl Plugin for ScenesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            SceneFlowPlugin::<GameScene>::default(),
            MenuPlugin::<GameScene>::default(),
            main_menu::MainMenuPlugin,
            mode_select::ModeSelectPlugin,
            settings_menu::SettingsMenuPlugin,
            keymap::KeymapScenePlugin,
            audio_settings::AudioSettingsPlugin,
            wheel::WheelScenePlugin,
            play::PlayScenePlugin,
            score::ScoreScenePlugin,
        ));
    }
}
