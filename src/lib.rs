pub mod core;
pub mod scenes;

use crate::core::config::GameConfig;
use crate::core::font::FontPlugin;
use crate::core::input::InputPlugin;
use crate::core::library::StepfileLibrary;
use crate::core::menu::MenuPlugin;
use crate::core::note_field::NoteFieldPlugin;
use crate::core::note_skin::NoteSkinPlugin;
use crate::core::scene_flow::SceneFlowPlugin;
use crate::core::settings::SettingsPlugin;
use crate::core::sfx::SfxPlugin;
use bevy::prelude::*;

pub fn run() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Rhythm".to_string(),
                        resolution: (1280, 720).into(),
                        // Inputs are judged at frame granularity; vsync would
                        // quantize presses to the display refresh (+0..16ms of
                        // one-sided timing error at 60Hz).
                        present_mode: bevy::window::PresentMode::AutoNoVsync,
                        ..default()
                    }),
                    ..default()
                })
                .set(bevy::log::LogPlugin {
                    // The text layouter's line segmenter ships without the
                    // CJK dictionary, so laying out a Japanese title warns
                    // "no segmentation model" on every relayout. The model
                    // only matters for wrapping CJK text mid-word, which
                    // never happens here — all our text is unwrapped.
                    filter: format!("{},icu_provider=error", bevy::log::DEFAULT_FILTER),
                    ..default()
                }),
        )
        .insert_resource(ClearColor(Color::srgb(0.04, 0.04, 0.07)))
        .insert_resource(GameConfig::load())
        .insert_resource(StepfileLibrary::scan())
        .add_plugins((
            FontPlugin,
            // The skin plugin loads the skin named by the settings.
            SettingsPlugin,
            NoteSkinPlugin,
            NoteFieldPlugin,
            InputPlugin,
            SfxPlugin,
            SceneFlowPlugin,
            MenuPlugin,
            scenes::ScenesPlugin,
        ))
        .add_systems(Startup, spawn_camera)
        .run();
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}
