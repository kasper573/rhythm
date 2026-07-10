use crate::core::assets::asset_root;
use crate::core::platform::platform;
use godot::classes::{FontFile, Label};
use godot::prelude::*;
use std::cell::RefCell;

const FONT_PATH: &str = "fonts/ipagp.ttf";

/// The game's single font, loaded once. Bundled with coverage for every
/// symbol that appears in the stepfile library's displayed names (Latin,
/// Greek, Cyrillic, kana, CJK, and common symbols) — the engine's default
/// font only covers basic Latin.
pub fn game_font() -> Gd<FontFile> {
    FONT.with_borrow_mut(|font| {
        font.get_or_insert_with(|| {
            let path = asset_root().join(FONT_PATH);
            let bytes = platform()
                .read_asset(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            let mut font = FontFile::new_gd();
            font.set_data(&PackedByteArray::from(bytes.as_slice()));
            font
        })
        .clone()
    })
}

thread_local! {
    static FONT: RefCell<Option<Gd<FontFile>>> = const { RefCell::new(None) };
}

/// Drops the cached font; the extension calls this on deinit, before the
/// engine tears its servers down.
pub fn drop_caches() {
    FONT.with_borrow_mut(|font| *font = None);
}

/// A label in the game font at a size and color — the one way text is made.
pub fn label(text: &str, size: f32, color: Color) -> Gd<Label> {
    let mut label = Label::new_alloc();
    label.set_text(text);
    label.add_theme_font_override("font", &game_font());
    label.add_theme_font_size_override("font_size", size.round() as i32);
    label.add_theme_color_override("font_color", color);
    label
}

/// Where a label's `position` anchors relative to its rendered size, as
/// fractions of it: `(0, 0)` is top-left, `(0.5, 0.5)` dead center.
#[derive(Debug, Clone, Copy)]
pub struct TextPivot(pub f32, pub f32);

impl TextPivot {
    pub const CENTER: TextPivot = TextPivot(0.5, 0.5);
    pub const CENTER_LEFT: TextPivot = TextPivot(0.0, 0.5);
    pub const BOTTOM_LEFT: TextPivot = TextPivot(0.0, 1.0);
}

/// Sizes the label to its content and places it so `pivot` lands on
/// `position` — free-floating text placement for canvas compositions.
/// Call again after changing the text.
pub fn place_label(label: &mut Gd<Label>, position: Vector2, pivot: TextPivot) {
    label.reset_size();
    let size = label.get_combined_minimum_size();
    label.set_position(position - Vector2::new(size.x * pivot.0, size.y * pivot.1));
}
