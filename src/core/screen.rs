//! The screen: the fixed design canvas, the camera stack that keeps it
//! visible, and the render layers visuals are composed on.

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, ScalingMode};
use bevy::prelude::*;
use bevy::ui::{IsDefaultUiCamera, UiScale};

/// The fixed logical screen size shared by the window and full-screen visuals.
pub const SCREEN_SIZE: Vec2 = Vec2::new(1280.0, 720.0);
/// The camera stack: the 2D world draws first, one 3D lane camera per
/// note field above it (layer and camera order are `LANE_LAYER_BASE` +
/// the field's lane), and the 2D overlay — receptor flashes, popups, and
/// all UI — on top of everything.
pub const LANE_LAYER_BASE: usize = 1;
pub const OVERLAY_LAYER: usize = 8;
pub const OVERLAY_CAMERA_ORDER: isize = 8;
pub const CLEAR_COLOR: Color = Color::srgb(0.04, 0.04, 0.07);

/// The world rect the AutoMin canvas camera shows in `window`: the whole
/// design canvas plus whatever extra the window's aspect reveals.
pub fn visible_world_size(window: &Window) -> Vec2 {
    let size = Vec2::new(window.width().max(1.0), window.height().max(1.0));
    size * (SCREEN_SIZE.x / size.x).max(SCREEN_SIZE.y / size.y)
}

/// A world position as a BSN fragment.
pub fn at(x: f32, y: f32, z: f32) -> impl Scene {
    let translation = Vec3::new(x, y, z);
    bsn! {
        Transform { translation: {translation} }
    }
}

/// Marks a sprite that must always cover the whole viewport. The camera
/// keeps the full 1280x720 canvas visible, so windows with a different
/// aspect see world beyond the canvas — covering sprites are resized to
/// the visible world rect every frame instead of the fixed canvas.
#[derive(Component, Default, Clone)]
pub struct ViewportCover;

/// The game is designed on a fixed 1280x720 canvas and scales uniformly
/// with the window: the cameras keep the whole canvas visible and the UI
/// follows the same factor, so world and UI grow together. The axis the
/// window has spare space on simply sees a little more room.
pub struct ScreenPlugin;

impl Plugin for ScreenPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(CLEAR_COLOR))
            .add_systems(Startup, spawn_cameras)
            .add_systems(Update, (scale_ui_to_window, size_viewport_covers));
    }
}

/// Two 2D cameras bracket the note fields' lane cameras (see the stepfile
/// player prefab): the world below them, the overlay — flashes, popups,
/// and all UI — above them.
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

fn size_viewport_covers(
    windows: Query<&Window>,
    mut sprites: Query<&mut Sprite, With<ViewportCover>>,
) {
    let Ok(window) = windows.single() else { return };
    let visible = visible_world_size(window);
    for mut sprite in &mut sprites {
        if sprite.custom_size != Some(visible) {
            sprite.custom_size = Some(visible);
        }
    }
}
