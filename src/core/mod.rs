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
pub mod scene_flow;
pub mod settings;
pub mod sfx;
pub mod stepfile;
pub mod tick_track;
pub mod units;
pub mod video;

use bevy::prelude::*;

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
