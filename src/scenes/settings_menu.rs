use crate::core::input::{Actions, GameAction};
use crate::core::menu::{MenuSelected, spawn_menu};
use crate::core::sfx::{PlaySfx, Sfx};
use crate::scenes::{GameScene, SceneFade, scene_accepts_input};
use bevy::prelude::*;

pub struct SettingsMenuPlugin;

impl Plugin for SettingsMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameScene::SettingsMenu), enter)
            .add_systems(
                Update,
                (handle_selection, handle_cancel)
                    .run_if(in_state(GameScene::SettingsMenu).and_then(scene_accepts_input)),
            );
    }
}

fn enter(mut commands: Commands) {
    spawn_menu(
        &mut commands,
        GameScene::SettingsMenu,
        "Settings",
        &["Configure keymap"],
    );
}

fn handle_selection(mut selected: MessageReader<MenuSelected>, mut fade: ResMut<SceneFade>) {
    for _ in selected.read() {
        fade.begin(GameScene::Keymap);
    }
}

fn handle_cancel(actions: Actions, mut fade: ResMut<SceneFade>, mut sfx: MessageWriter<PlaySfx>) {
    if actions.just_pressed(GameAction::Cancel) {
        sfx.write(PlaySfx(Sfx::Cancel));
        fade.begin(GameScene::MainMenu);
    }
}
