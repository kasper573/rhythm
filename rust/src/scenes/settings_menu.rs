use crate::core::input::{Actions, GameAction};
use crate::core::player::PlayerId;
use crate::core::sfx::Sfx;
use crate::nodes::menu::{Menu, MenuOptions};
use crate::scenes::{
    GameScene, change_scene, play_default_bgm, scene_accepts_input, spawn_default_background,
};
use godot::classes::control::LayoutPreset;
use godot::classes::{Control, IControl};
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base=Control)]
pub struct SettingsMenuScene {
    base: Base<Control>,
}

#[godot_api]
impl SettingsMenuScene {
    pub fn instantiate() -> Gd<SettingsMenuScene> {
        let mut scene = SettingsMenuScene::new_alloc();
        scene.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        play_default_bgm();
        spawn_default_background(&mut scene.clone().upcast::<Control>());
        let menu = Menu::instantiate(MenuOptions {
            title: "Settings".to_string(),
            items: vec!["Configure keymap".to_string(), "Audio settings".to_string()],
        });
        scene.add_child(&menu);
        menu.signals()
            .selected()
            .connect_other(&scene, SettingsMenuScene::on_selected);
        scene
    }

    fn on_selected(&mut self, index: i64) {
        change_scene(match index {
            0 => GameScene::Keymap,
            _ => GameScene::AudioSettings,
        });
    }
}

#[godot_api]
impl IControl for SettingsMenuScene {
    fn init(base: Base<Control>) -> SettingsMenuScene {
        SettingsMenuScene { base }
    }

    fn process(&mut self, _delta: f64) {
        if scene_accepts_input() && Actions::just_pressed(GameAction::cancel(PlayerId::P1)) {
            Sfx::Cancel.play();
            change_scene(GameScene::MainMenu);
        }
    }
}
