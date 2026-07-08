use bevy::prelude::*;
use bevy::text::{FontSize, FontSourceTemplate};

const FONT_PATH: &str = "fonts/ipagp.ttf";

/// The game's single font at a size, as a BSN fragment merged into text
/// entities. Bundled with coverage for every symbol that appears in the
/// stepfile library's displayed names (Latin, Greek, Cyrillic, kana, CJK,
/// and common symbols) — the engine's default font only covers basic Latin.
pub fn game_font(size: f32) -> impl Scene {
    bsn! {
        TextFont {
            font: FontSourceTemplate::Handle(FONT_PATH),
            font_size: {FontSize::from(size)},
        }
    }
}
