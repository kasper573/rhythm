pub mod assets;
pub mod config;
pub mod font;
pub mod health_vial;
pub mod high_scores;
pub mod input;
pub mod jsonc;
pub mod library;
pub mod menu;
pub mod note_field;
pub mod note_skin;
pub mod persist;
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

/// A world position as a BSN fragment.
pub fn at(x: f32, y: f32, z: f32) -> impl Scene {
    let translation = Vec3::new(x, y, z);
    bsn! {
        Transform { translation: {translation} }
    }
}

/// A rotated world position as a BSN fragment.
pub fn oriented(x: f32, y: f32, z: f32, rotation: Quat) -> impl Scene {
    let translation = Vec3::new(x, y, z);
    bsn! {
        Transform { translation: {translation}, rotation: {rotation} }
    }
}

/// The fixed logical screen size shared by the window and full-screen visuals.
pub const SCREEN_SIZE: Vec2 = Vec2::new(1280.0, 720.0);
pub const CLEAR_COLOR: Color = Color::srgb(0.04, 0.04, 0.07);
