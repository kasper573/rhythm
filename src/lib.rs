pub mod core;
pub mod scenes;

use crate::core::config::GameConfig;
use crate::core::health_vial::HealthVialPlugin;
use crate::core::input::InputPlugin;
use crate::core::library::StepfileLibrary;
use crate::core::note_field::NoteFieldPlugin;
use crate::core::note_skin::NoteSkinPlugin;
use crate::core::settings::SettingsPlugin;
use crate::core::sfx::SfxPlugin;
use crate::core::{CLEAR_COLOR, SCREEN_SIZE};
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
                        // Inputs are graded at frame granularity; vsync would
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
        .insert_resource(ClearColor(CLEAR_COLOR))
        .insert_resource(config)
        .insert_resource(StepfileLibrary::scan())
        .add_plugins((
            settings_plugin,
            NoteSkinPlugin,
            NoteFieldPlugin,
            HealthVialPlugin,
            InputPlugin,
            SfxPlugin,
            scenes::ScenesPlugin,
        ))
        .add_systems(Startup, camera.spawn())
        .run();
}

fn camera() -> impl Scene {
    bsn! { Camera2d }
}
