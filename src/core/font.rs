use bevy::prelude::*;

/// The game's single UI font, bundled with coverage for every symbol that
/// appears in the stepfile library's displayed names (Latin, Greek, Cyrillic,
/// kana, CJK, and common symbols) — the engine's default font only covers
/// basic Latin.
#[derive(Resource)]
pub struct GameFont(pub Handle<Font>);

impl GameFont {
    pub fn sized(&self, size: f32) -> TextFont {
        TextFont {
            font: self.0.clone().into(),
            font_size: size.into(),
            ..default()
        }
    }
}

pub struct FontPlugin;

impl Plugin for FontPlugin {
    fn build(&self, app: &mut App) {
        let font = app
            .world()
            .resource::<AssetServer>()
            .load("fonts/ipagp.ttf");
        app.insert_resource(GameFont(font));
    }
}
