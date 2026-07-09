use crate::core::input::{Actions, GameAction};
use crate::core::menu::{MenuSelected, spawn_menu};
use crate::core::player::PlayerId;
use crate::core::sfx::{PlaySfx, Sfx};
use crate::scenes::{
    GameScene, SceneFade, play_default_bgm, scene_accepts_input, spawn_default_background,
};
use bevy::prelude::*;

pub(super) struct SettingsMenuPlugin;

impl Plugin for SettingsMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(GameScene::SettingsMenu),
            (enter, play_default_bgm, spawn_default_background),
        )
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
        &["Configure keymap", "Audio settings"],
    );
}

fn handle_selection(mut selected: MessageReader<MenuSelected>, mut fade: ResMut<SceneFade>) {
    for selection in selected.read() {
        fade.begin(match selection.index {
            0 => GameScene::Keymap,
            _ => GameScene::AudioSettings,
        });
    }
}

fn handle_cancel(actions: Actions, mut fade: ResMut<SceneFade>, mut sfx: MessageWriter<PlaySfx>) {
    if actions.just_pressed(GameAction::cancel(PlayerId::P1)) {
        sfx.write(PlaySfx(Sfx::Cancel));
        fade.begin(GameScene::MainMenu);
    }
}
