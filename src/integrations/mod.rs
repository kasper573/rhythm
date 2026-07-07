//! Tiny pieces of wiring that couple core mechanisms together.

use crate::core::note_skin::{ActiveNoteSkin, load_note_skin, scan_note_skins};
use crate::core::settings::Settings;
use bevy::prelude::*;

/// Wires the note skin to the settings: the active skin is the one the
/// settings name, loaded at startup and reloaded whenever they change (the
/// player options scene edits them). Requires [`Settings`] to already be
/// inserted.
pub struct SettingsNoteSkinPlugin;

impl Plugin for SettingsNoteSkinPlugin {
    fn build(&self, app: &mut App) {
        let name = app
            .world()
            .resource::<Settings>()
            .stepfile
            .note_skin
            .clone();
        let skin = app.world_mut().resource_scope(
            |world, mut layouts: Mut<Assets<TextureAtlasLayout>>| {
                load_note_skin(world.resource::<AssetServer>(), &mut layouts, &name)
            },
        );
        app.insert_resource(skin)
            .insert_resource(scan_note_skins())
            .add_systems(Update, reload_changed_skin);
    }
}

fn reload_changed_skin(
    settings: Res<Settings>,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut skin: ResMut<ActiveNoteSkin>,
) {
    if !settings.is_changed() || skin.name == settings.stepfile.note_skin {
        return;
    }
    *skin = load_note_skin(&asset_server, &mut layouts, &settings.stepfile.note_skin);
}
