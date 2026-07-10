pub mod core;
#[cfg(not(target_arch = "wasm32"))]
pub mod native;
pub mod prefabs;
pub mod profiling;
pub mod scenes;
#[cfg(target_arch = "wasm32")]
pub mod web;

use crate::core::audio::AudioPlugin;
use crate::core::config::GameConfig;
use crate::core::high_scores::HighScoresPlugin;
use crate::core::library::StepfileLibrary;
use crate::core::player::PlayMode;
use crate::core::screen::{SCREEN_SIZE, ScreenPlugin};
use crate::core::settings::SettingsPlugin;
use crate::core::sfx::SfxPlugin;
use crate::core::stepfile::MusicPlayerPlugin;
use crate::prefabs::fps_overlay::FpsOverlayPlugin;
use crate::prefabs::health_vial::HealthVialPlugin;
use crate::prefabs::media_cover::MediaCoverPlugin;
use crate::prefabs::stepfile_player::StepfilePlayerPlugin;
use bevy::prelude::*;

pub fn run(platform: impl core::platform::Platform + 'static) {
    app(platform).run();
}

/// The complete game, exactly as [`run`] plays it — also the base the
/// bench binary drives, so measurements exercise the shipped code paths.
pub fn app(platform: impl core::platform::Platform + 'static) -> App {
    core::platform::install(platform);
    let config = GameConfig::load();
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Rhythm".to_string(),
                    resolution: SCREEN_SIZE.as_uvec2().into(),
                    // Inputs are graded at frame granularity; vsync would
                    // quantize presses to the display refresh (+0..16ms of
                    // one-sided timing error at 60Hz).
                    present_mode: bevy::window::PresentMode::AutoNoVsync,
                    fit_canvas_to_parent: true,
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
                custom_layer: profiling::layer,
                ..default()
            }),
    )
    .insert_resource(config)
    .insert_resource(StepfileLibrary::scan())
    .init_resource::<PlayMode>()
    .add_plugins((
        ScreenPlugin,
        SettingsPlugin,
        AudioPlugin,
        StepfilePlayerPlugin,
        HealthVialPlugin,
        FpsOverlayPlugin,
        MediaCoverPlugin,
        HighScoresPlugin,
        MusicPlayerPlugin,
        SfxPlugin,
        scenes::ScenesPlugin,
    ));
    app
}
