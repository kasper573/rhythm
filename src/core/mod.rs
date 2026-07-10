pub mod assets;
pub mod audio;
pub mod config;
pub mod font;
pub mod high_scores;
pub mod input;
pub mod jsonc;
pub mod library;
pub mod persist;
pub mod platform;
pub mod player;
pub mod scene_flow;
pub mod settings;
pub mod sfx;
pub mod stepfile;
pub mod tick_track;
pub mod units;
pub mod video;

use bevy::prelude::*;

/// Marks a sprite that must always cover the whole viewport. The camera
/// keeps the full 1280x720 canvas visible, so windows with a different
/// aspect see world beyond the canvas — covering sprites are resized to
/// the visible world rect every frame instead of the fixed canvas.
#[derive(Component, Default, Clone)]
pub struct ViewportCover;

pub fn size_viewport_covers(
    windows: Query<&Window>,
    mut sprites: Query<&mut Sprite, With<ViewportCover>>,
) {
    let Ok(window) = windows.single() else { return };
    let size = Vec2::new(window.width(), window.height());
    if size.x <= 0.0 || size.y <= 0.0 {
        return;
    }
    let visible = size * (SCREEN_SIZE.x / size.x).max(SCREEN_SIZE.y / size.y);
    for mut sprite in &mut sprites {
        if sprite.custom_size != Some(visible) {
            sprite.custom_size = Some(visible);
        }
    }
}

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
