use crate::core::input::{Actions, GameAction};
use crate::core::player::{PlayMode, PlayerId};
use crate::core::sfx::Sfx;
use crate::game::Game;
use crate::nodes::menu::{Menu, MenuOptions};
use crate::scenes::{
    GameScene, change_scene, play_default_bgm, scene_accepts_input, spawn_default_background,
};
use godot::classes::control::LayoutPreset;
use godot::classes::{Control, IControl};
use godot::prelude::*;
use strum::IntoEnumIterator;

#[derive(GodotClass)]
#[class(base=Control)]
pub struct ModeSelectScene {
    base: Base<Control>,
}

#[godot_api]
impl ModeSelectScene {
    pub fn instantiate() -> Gd<ModeSelectScene> {
        let mut scene = ModeSelectScene::new_alloc();
        scene.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        play_default_bgm();
        spawn_default_background(&mut scene.clone().upcast::<Control>());
        let menu = Menu::instantiate(MenuOptions {
            title: "Select Mode".to_string(),
            items: PlayMode::iter()
                .map(|mode| mode.label().to_string())
                .collect(),
        });
        scene.add_child(&menu);
        menu.signals()
            .selected()
            .connect_other(&scene, ModeSelectScene::on_selected);
        scene
    }

    fn on_selected(&mut self, index: i64) {
        let Some(picked) = PlayMode::iter().nth(index as usize) else {
            return;
        };
        Game::singleton().bind_mut().set_play_mode(picked);
        change_scene(GameScene::Wheel);
    }
}

#[godot_api]
impl IControl for ModeSelectScene {
    fn init(base: Base<Control>) -> ModeSelectScene {
        ModeSelectScene { base }
    }

    fn process(&mut self, _delta: f64) {
        if scene_accepts_input() && Actions::just_pressed(GameAction::cancel(PlayerId::P1)) {
            Sfx::Cancel.play();
            change_scene(GameScene::MainMenu);
        }
    }
}
