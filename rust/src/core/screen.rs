use godot::classes::Node;
use godot::prelude::*;

/// The fixed logical design canvas. The project's `canvas_items` stretch
/// with `expand` aspect keeps it fully visible and uniformly scaled;
/// windows with a different aspect see extra canvas past it.
pub const SCREEN_SIZE: Vector2 = Vector2::new(1280.0, 720.0);

pub const CLEAR_COLOR: Color = Color::from_rgb(0.04, 0.04, 0.07);

/// The canvas rect the window currently shows: the whole design canvas
/// plus whatever extra the window's aspect reveals. Layout that hugs
/// screen edges or centers on the screen derives from this every frame.
pub fn visible_rect(node: &Node) -> Rect2 {
    node.get_viewport()
        .map(|viewport| viewport.get_visible_rect())
        .unwrap_or(Rect2::new(Vector2::ZERO, SCREEN_SIZE))
}

/// The visible canvas center — where the design canvas' center sits.
pub fn visible_center(node: &Node) -> Vector2 {
    let rect = visible_rect(node);
    rect.position + rect.size / 2.0
}

/// A canvas blend factor compensated for the 2D pipeline blending in sRGB
/// space: the game's look was designed on a linear-blending renderer where
/// the same factor mixes brighter, so partial alphas are encoded with the
/// sRGB exponent to restore the designed brightness (the grade shader's
/// bloom does the same). Apply to the surviving side of a blend: a wash at
/// `linear_blend(0.25)`, a black fade covering at `1 - linear_blend(1 - t)`.
pub fn linear_blend(factor: f32) -> f32 {
    factor.clamp(0.0, 1.0).powf(1.0 / 2.2)
}
