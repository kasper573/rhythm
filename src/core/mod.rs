pub mod assets;
pub mod config;

/// The fixed logical screen size shared by the window and full-screen visuals.
pub const SCREEN_SIZE: bevy::math::Vec2 = bevy::math::Vec2::new(1280.0, 720.0);
pub mod font;
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
