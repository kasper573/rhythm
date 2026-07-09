pub mod core;
#[cfg(not(target_arch = "wasm32"))]
pub mod native;
pub mod profiling;
pub mod scenes;
#[cfg(target_arch = "wasm32")]
pub mod web;

use crate::core::audio::AudioPlugin;
use crate::core::config::GameConfig;
use crate::core::health_vial::HealthVialPlugin;
use crate::core::high_scores::HighScoresPlugin;
use crate::core::input::InputPlugin;
use crate::core::library::StepfileLibrary;
use crate::core::note_field::NoteFieldPlugin;
use crate::core::note_skin::NoteSkinPlugin;
use crate::core::player::PlayMode;
use crate::core::settings::SettingsPlugin;
use crate::core::sfx::SfxPlugin;
use crate::core::stepfile::MusicPlayerPlugin;
use crate::core::{
    CLEAR_COLOR, OVERLAY_CAMERA_ORDER, OVERLAY_LAYER, SCREEN_SIZE, size_viewport_covers,
};
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, ScalingMode};
use bevy::prelude::*;
use bevy::ui::{IsDefaultUiCamera, UiScale};

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
    .insert_resource(ClearColor(CLEAR_COLOR))
    .insert_resource(config)
    .insert_resource(StepfileLibrary::scan())
    .init_resource::<PlayMode>()
    .add_plugins((
        SettingsPlugin,
        AudioPlugin,
        NoteSkinPlugin,
        NoteFieldPlugin,
        HealthVialPlugin,
        HighScoresPlugin,
        MusicPlayerPlugin,
        InputPlugin,
        SfxPlugin,
        scenes::ScenesPlugin,
    ))
    .add_systems(Startup, spawn_cameras)
    .add_systems(Update, (scale_ui_to_window, size_viewport_covers));
    app
}

/// The game is designed on a fixed 1280x720 canvas and scales uniformly
/// with the window: the cameras keep the whole canvas visible and the UI
/// follows the same factor, so world and UI grow together. The axis the
/// window has spare space on simply sees a little more room.
///
/// Two 2D cameras bracket the note fields' lane cameras (see
/// `core::note_field`): the world below them, the overlay — flashes,
/// popups, and all UI — above them.
fn spawn_cameras(mut commands: Commands) {
    commands
        .spawn_scene(bsn! { Camera2d })
        .insert(canvas_projection());
    commands.spawn_scene(bsn! { Camera2d }).insert((
        canvas_projection(),
        Camera {
            order: OVERLAY_CAMERA_ORDER,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        RenderLayers::layer(OVERLAY_LAYER),
        IsDefaultUiCamera,
    ));
}

fn canvas_projection() -> Projection {
    Projection::Orthographic(OrthographicProjection {
        scaling_mode: ScalingMode::AutoMin {
            min_width: SCREEN_SIZE.x,
            min_height: SCREEN_SIZE.y,
        },
        ..OrthographicProjection::default_2d()
    })
}

fn scale_ui_to_window(windows: Query<&Window, Changed<Window>>, mut ui_scale: ResMut<UiScale>) {
    let Ok(window) = windows.single() else { return };
    let scale = (window.width() / SCREEN_SIZE.x).min(window.height() / SCREEN_SIZE.y);
    if scale > 0.0 && ui_scale.0 != scale {
        ui_scale.0 = scale;
    }
}
