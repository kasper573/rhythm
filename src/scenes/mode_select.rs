use crate::core::input::{Actions, GameAction};
use crate::core::menu::{MenuSelected, spawn_menu};
use crate::core::player::{PlayMode, PlayerId};
use crate::core::sfx::{PlaySfx, Sfx};
use crate::scenes::{
    GameScene, SceneFade, play_default_bgm, scene_accepts_input, spawn_default_background,
};
use bevy::prelude::*;
use strum::IntoEnumIterator;

pub(super) struct ModeSelectPlugin;

impl Plugin for ModeSelectPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(GameScene::ModeSelect),
            (enter, play_default_bgm, spawn_default_background),
        )
        .add_systems(
            Update,
            (handle_selection, handle_cancel)
                .run_if(in_state(GameScene::ModeSelect).and_then(scene_accepts_input)),
        );
    }
}

fn enter(mut commands: Commands) {
    let labels: Vec<&str> = PlayMode::iter().map(PlayMode::label).collect();
    spawn_menu(&mut commands, GameScene::ModeSelect, "Select Mode", &labels);
}

fn handle_selection(
    mut selected: MessageReader<MenuSelected>,
    mut mode: ResMut<PlayMode>,
    mut fade: ResMut<SceneFade>,
) {
    for selection in selected.read() {
        let Some(picked) = PlayMode::iter().nth(selection.index) else {
            continue;
        };
        *mode = picked;
        fade.begin(GameScene::FileSelect);
    }
}

fn handle_cancel(actions: Actions, mut fade: ResMut<SceneFade>, mut sfx: MessageWriter<PlaySfx>) {
    if actions.just_pressed(GameAction::cancel(PlayerId::P1)) {
        sfx.write(PlaySfx(Sfx::Cancel));
        fade.begin(GameScene::MainMenu);
    }
}
