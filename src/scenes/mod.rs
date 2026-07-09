pub mod audio_settings;
pub mod file_player;
pub mod file_select;
pub mod keymap;
pub mod main_menu;
pub mod mode_select;
pub mod score;
pub mod settings_menu;

use crate::core::assets::asset_server_path;
use crate::core::library::{StepfileLibrary, is_video_file};
use crate::core::menu::MenuPlugin;
use crate::core::scene_flow::{SceneFlowPlugin, SpawnScoped};
use crate::core::units::Seconds;
use crate::core::video::VideoStream;
use crate::core::{SCREEN_SIZE, ViewportCover, at};
use bevy::prelude::*;
use bevy::sprite::{SpriteImageMode, SpriteScalingMode};

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GameScene {
    #[default]
    MainMenu,
    ModeSelect,
    SettingsMenu,
    Keymap,
    AudioSettings,
    FileSelect,
    FilePlayer,
    Score,
}

pub type SceneFade = crate::core::scene_flow::SceneFade<GameScene>;

/// The default BGM's background — its looping video, dimmed — behind the
/// entered scene's UI. Registered on the `OnEnter` of scenes that want
/// it, torn down with the scene; the scene fade masks the remount.
#[derive(Component, Default, Clone)]
struct DefaultSceneBackground;

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
    let (image, stream) = if is_video_file(&path.to_string_lossy()) {
        match VideoStream::open(&path, Seconds(time.elapsed_secs_f64()), true, &mut images) {
            Ok(stream) => (stream.image.clone(), Some(stream)),
            Err(error) => {
                warn!(
                    "scene background unavailable for {}: {error}",
                    path.display()
                );
                return;
            }
        }
    } else {
        let Some(asset) = asset_server_path(&path) else {
            return;
        };
        (asset_server.load(asset), None)
    };
    let mut background = commands.spawn_scoped(
        *scene.get(),
        bsn! {
            DefaultSceneBackground
            ViewportCover
            Sprite {
                image: {image},
                // Dimmed so the scene's text stays readable in front.
                color: Color::srgb(0.5, 0.5, 0.5),
                custom_size: {Some(SCREEN_SIZE)},
                image_mode: {SpriteImageMode::Scale(SpriteScalingMode::FillCenter)},
            }
            at(0.0, 0.0, -10.0)
        },
    );
    // The stream owns a live decoder, so it cannot be a cloneable
    // template value.
    if let Some(stream) = stream {
        background.insert(stream);
    }
}

/// Keeps the scene backgrounds' videos decoding on wall time.
fn stream_default_backgrounds(
    time: Res<Time>,
    mut images: ResMut<Assets<Image>>,
    mut videos: Query<&mut VideoStream, With<DefaultSceneBackground>>,
) {
    let now = Seconds(time.elapsed_secs_f64());
    for mut video in &mut videos {
        video.update(now, &mut images);
    }
}

/// Scenes without music of their own start the default BGM on enter; the
/// player keeps it running across such scenes uninterrupted.
fn play_default_bgm(
    library: Res<crate::core::library::StepfileLibrary>,
    mut music: ResMut<crate::core::stepfile::MusicPlayer>,
) {
    music.play(library.default_bgm.bgm());
}

fn scene_accepts_input(fade: Res<SceneFade>) -> bool {
    fade.accepts_input()
}

pub(crate) struct ScenesPlugin;

impl Plugin for ScenesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, stream_default_backgrounds)
            .add_plugins((
                SceneFlowPlugin::<GameScene>::default(),
                MenuPlugin::<GameScene>::default(),
                main_menu::MainMenuPlugin,
                mode_select::ModeSelectPlugin,
                settings_menu::SettingsMenuPlugin,
                keymap::KeymapScenePlugin,
                audio_settings::AudioSettingsPlugin,
                file_select::FileSelectPlugin,
                file_player::FilePlayerPlugin,
                score::ScoreScenePlugin,
            ));
    }
}
