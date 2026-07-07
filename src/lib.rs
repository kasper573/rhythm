pub mod core;
pub mod integrations;
pub mod scenes;

use crate::core::SCREEN_SIZE;
use crate::core::config::GameConfig;
use crate::core::font::FontPlugin;
use crate::core::input::InputPlugin;
use crate::core::library::StepfileLibrary;
use crate::core::note_field::NoteFieldPlugin;
use crate::core::settings::SettingsPlugin;
use crate::core::sfx::SfxPlugin;
use crate::integrations::SettingsNoteSkinPlugin;
use bevy::prelude::*;

pub fn run() {
    let config = GameConfig::load();
    let settings_plugin = SettingsPlugin {
        default_stepfile: config.default_stepfile_options.clone(),
    };
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Rhythm".to_string(),
                        resolution: SCREEN_SIZE.as_uvec2().into(),
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
        .insert_resource(config)
        .insert_resource(StepfileLibrary::scan())
        .add_plugins((
            FontPlugin,
            settings_plugin,
            SettingsNoteSkinPlugin,
            NoteFieldPlugin,
            InputPlugin,
            SfxPlugin,
            scenes::ScenesPlugin,
        ))
        .add_systems(Startup, spawn_camera)
        .run();
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}
