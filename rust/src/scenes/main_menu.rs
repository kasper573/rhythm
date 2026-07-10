use crate::nodes::menu::{Menu, MenuOptions};
use crate::scenes::{GameScene, change_scene, play_default_bgm, spawn_default_background};
use godot::classes::control::LayoutPreset;
use godot::classes::{Control, IControl};
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base=Control)]
pub struct MainMenuScene {
    base: Base<Control>,
}

#[godot_api]
impl MainMenuScene {
    pub fn instantiate() -> Gd<MainMenuScene> {
        let mut scene = MainMenuScene::new_alloc();
        scene.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        play_default_bgm();
        spawn_default_background(&mut scene.clone().upcast::<Control>());
        let menu = Menu::instantiate(MenuOptions {
            title: "Rhythm".to_string(),
            items: vec![
                "Start".to_string(),
                "Settings".to_string(),
                "Quit".to_string(),
            ],
        });
        scene.add_child(&menu);
        menu.signals()
            .selected()
            .connect_other(&scene, MainMenuScene::on_selected);
        scene
    }

    fn on_selected(&mut self, index: i64) {
        match index {
            0 => change_scene(GameScene::ModeSelect),
            1 => change_scene(GameScene::SettingsMenu),
            _ => {
                self.base().get_tree().quit();
            }
        }
    }
}

#[godot_api]
impl IControl for MainMenuScene {
    fn init(base: Base<Control>) -> MainMenuScene {
        MainMenuScene { base }
    }
}
