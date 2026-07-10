use crate::core::config::{GameConfig, config};
use crate::core::font::label;
use crate::core::input::{GameAction, GameKey};
use crate::core::settings::Settings;
use crate::core::sfx::Sfx;
use crate::core::units::Seconds;
use crate::nodes::menu::{INACTIVE_COLOR, TITLE_COLOR};
use crate::scenes::{
    GameScene, change_scene, play_default_bgm, scene_accepts_input, spawn_default_background,
};
use godot::classes::control::LayoutPreset;
use godot::classes::{
    CenterContainer, Control, IControl, Input, InputEvent, InputEventKey, Label, VBoxContainer,
};
use godot::global::HorizontalAlignment;
use godot::prelude::*;
use strum::{EnumCount, IntoEnumIterator};

/// The keymap editor. Its own navigation resolves through the config's
/// DEFAULT keymap, never the live one it edits: however broken the stored
/// bindings get, this scene stays operable to repair them.
#[derive(GodotClass)]
#[class(base=Control)]
pub struct KeymapScene {
    active: usize,
    /// Which action the rebind prompt is listening for, when open.
    prompt: Option<GameAction>,
    /// The prompt was closed by this frame's key event; the same key must
    /// not double as navigation or scene exit in this frame's process.
    prompt_just_closed: bool,
    /// The ¤P1Cancel¤ gesture: holding resets the active row's binding to
    /// its default, a shorter tap leaves the scene. Only presses that began
    /// while no prompt was open are armed.
    cancel_armed: bool,
    cancel_held: Seconds,
    nav_before: [bool; 4],
    action_labels: Vec<Gd<Label>>,
    binding_labels: Vec<Gd<Label>>,
    prompt_label: Option<Gd<Label>>,
    base: Base<Control>,
}

/// The table is centered by a fixed-size box, never by content-driven
/// centering: content sits within the box, so neither the prompt appearing
/// nor a binding's width changing can shift it.
const TABLE_WIDTH: f32 = 560.0;
const TABLE_HEIGHT: f32 = 620.0;

/// The title rides above the table at a fixed distance from the top edge.
const TITLE_TOP: f32 = 44.0;

/// A fixed key column keeps the grid's own width constant as bindings
/// change length, so the centering above never recomputes.
const KEY_COLUMN_WIDTH: f32 = 200.0;

/// The key column's trailing slack (short keys don't fill it) pulls the
/// visible ink left of the box's true center; a positive inset nudges the
/// box right by half this to rebalance it.
const CENTER_BIAS: f32 = 60.0;

/// How long ¤P1Cancel¤ must be held to reset instead of leave.
const RESET_HOLD: Seconds = Seconds(0.5);

/// The scene's fixed navigation actions, resolved through the defaults:
/// up, down, select, cancel.
const NAV_ACTIONS: [GameAction; 4] = [
    GameAction::P1Up,
    GameAction::P1Down,
    GameAction::P1Select,
    GameAction::P1Cancel,
];

#[godot_api]
impl KeymapScene {
    pub fn instantiate() -> Gd<KeymapScene> {
        let mut scene = KeymapScene::new_alloc();
        scene.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        play_default_bgm();
        spawn_default_background(&mut scene.clone().upcast::<Control>());

        let mut title = label("Keymap", 44.0, TITLE_COLOR);
        title.set_horizontal_alignment(HorizontalAlignment::CENTER);
        title.set_anchors_and_offsets_preset(LayoutPreset::TOP_WIDE);
        title.set_offset(godot::builtin::Side::TOP, TITLE_TOP);
        scene.add_child(&title);

        // Fixed-size box centered on the screen: its position is immune to
        // what its content does.
        let mut table = Control::new_alloc();
        table.set_anchors_preset(LayoutPreset::CENTER);
        table.set_size(Vector2::new(TABLE_WIDTH, TABLE_HEIGHT));
        table.set_position(Vector2::new(
            -TABLE_WIDTH / 2.0 + CENTER_BIAS / 2.0,
            -TABLE_HEIGHT / 2.0,
        ));
        let mut grid_center = CenterContainer::new_alloc();
        grid_center.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        let mut grid = godot::classes::HBoxContainer::new_alloc();
        grid.add_theme_constant_override("separation", 48);
        let mut labels_column = VBoxContainer::new_alloc();
        labels_column.add_theme_constant_override("separation", 2);
        let mut keys_column = VBoxContainer::new_alloc();
        keys_column.add_theme_constant_override("separation", 2);
        keys_column.set_custom_minimum_size(Vector2::new(KEY_COLUMN_WIDTH, 0.0));

        let settings = Settings::singleton();
        let mut action_labels = Vec::new();
        let mut binding_labels = Vec::new();
        for action in GameAction::iter() {
            let name = label(action.label(), 19.0, INACTIVE_COLOR);
            labels_column.add_child(&name);
            action_labels.push(name);
            let key = label(
                &key_label(action, &settings.bind().machine().keymap, config()),
                19.0,
                INACTIVE_COLOR,
            );
            keys_column.add_child(&key);
            binding_labels.push(key);
        }
        grid.add_child(&labels_column);
        grid.add_child(&keys_column);
        grid_center.add_child(&grid);
        table.add_child(&grid_center);
        scene.add_child(&table);

        let mut prompt = label("", 22.0, Color::from_rgb(0.4, 0.9, 0.6));
        prompt.set_horizontal_alignment(HorizontalAlignment::CENTER);
        prompt.set_anchors_and_offsets_preset(LayoutPreset::BOTTOM_WIDE);
        prompt.set_offset(godot::builtin::Side::TOP, -80.0);
        prompt.set_offset(godot::builtin::Side::BOTTOM, -56.0);
        scene.add_child(&prompt);

        let reset_help = format!(
            "Hold {} to reset selected key to default",
            default_key(config(), GameAction::P1Cancel).name()
        );
        let mut help = label(&reset_help, 18.0, INACTIVE_COLOR);
        help.set_horizontal_alignment(HorizontalAlignment::CENTER);
        help.set_anchors_and_offsets_preset(LayoutPreset::BOTTOM_WIDE);
        help.set_offset(godot::builtin::Side::TOP, -40.0);
        help.set_offset(godot::builtin::Side::BOTTOM, -16.0);
        scene.add_child(&help);

        let mut bound = scene.bind_mut();
        bound.action_labels = action_labels;
        bound.binding_labels = binding_labels;
        bound.prompt_label = Some(prompt);
        drop(bound);
        scene.bind_mut().refresh_rows();
        scene
    }

    fn navigate(&mut self, back: bool) {
        let len = GameAction::COUNT;
        self.active = if back {
            (self.active + len - 1) % len
        } else {
            (self.active + 1) % len
        };
        Sfx::Navigate.play();
        self.refresh_rows();
    }

    fn open_prompt(&mut self) {
        self.prompt = GameAction::iter().nth(self.active);
        Sfx::Select.play();
        self.refresh_prompt();
    }

    fn refresh_rows(&mut self) {
        let settings = Settings::singleton();
        let keymap = settings.bind().machine().keymap.clone();
        for (index, action) in GameAction::iter().enumerate() {
            let color = if index == self.active {
                Color::WHITE
            } else {
                INACTIVE_COLOR
            };
            self.action_labels[index].add_theme_color_override("font_color", color);
            let binding = &mut self.binding_labels[index];
            binding.set_text(&key_label(action, &keymap, config()));
            binding.add_theme_color_override("font_color", color);
        }
    }

    fn refresh_prompt(&mut self) {
        let text = match self.prompt {
            Some(action) => format!(
                "Press a key for \"{}\" ({} aborts)",
                action.label(),
                default_key(config(), GameAction::P1Cancel).name()
            ),
            None => String::new(),
        };
        if let Some(label) = &mut self.prompt_label {
            label.set_text(&text);
        }
    }

    /// The ¤P1Cancel¤ gesture between prompts: holding resets the active
    /// row's binding to its default, a shorter tap leaves the scene.
    fn cancel_gesture(&mut self, delta: f64, just_pressed: bool, just_released: bool) {
        if just_pressed {
            self.cancel_armed = true;
            self.cancel_held = Seconds::ZERO;
        }
        if !self.cancel_armed {
            return;
        }
        if Input::singleton().is_physical_key_pressed(default_key(config(), GameAction::P1Cancel).0)
        {
            self.cancel_held += Seconds(delta);
            if self.cancel_held >= RESET_HOLD {
                self.cancel_armed = false;
                if let Some(action) = GameAction::iter().nth(self.active) {
                    Settings::singleton()
                        .bind_mut()
                        .edit_machine(|machine| machine.keymap.reset(action));
                    Sfx::Select.play();
                    self.refresh_rows();
                }
            }
            return;
        }
        if just_released {
            self.cancel_armed = false;
            Sfx::Cancel.play();
            change_scene(GameScene::SettingsMenu);
        }
    }
}

#[godot_api]
impl IControl for KeymapScene {
    fn init(base: Base<Control>) -> KeymapScene {
        KeymapScene {
            active: 0,
            prompt: None,
            prompt_just_closed: false,
            cancel_armed: false,
            cancel_held: Seconds::ZERO,
            nav_before: [false; 4],
            action_labels: Vec::new(),
            binding_labels: Vec::new(),
            prompt_label: None,
            base,
        }
    }

    /// The rebind prompt captures raw key events; the press that closes it
    /// is flagged so this frame's process never doubles it as navigation.
    fn input(&mut self, event: Gd<InputEvent>) {
        let Some(action) = self.prompt else {
            return;
        };
        let Ok(key_event) = event.try_cast::<InputEventKey>() else {
            return;
        };
        if !key_event.is_pressed() || key_event.is_echo() {
            return;
        }
        let key = key_event.get_physical_keycode();
        if key == default_key(config(), GameAction::P1Cancel).0 {
            Sfx::Cancel.play();
        } else {
            Settings::singleton()
                .bind_mut()
                .edit_machine(|machine| machine.keymap.set(action, GameKey(key)));
            Sfx::Select.play();
        }
        self.prompt = None;
        self.prompt_just_closed = true;
        self.refresh_prompt();
        self.refresh_rows();
    }

    fn process(&mut self, delta: f64) {
        // Raw just-pressed edges for the scene's fixed default-keymap
        // navigation, immune to the live bindings this scene edits.
        let input = Input::singleton();
        let now: Vec<bool> = NAV_ACTIONS
            .iter()
            .map(|action| input.is_physical_key_pressed(default_key(config(), *action).0))
            .collect();
        let just = |index: usize, before: &[bool; 4]| now[index] && !before[index];
        let released = |index: usize, before: &[bool; 4]| !now[index] && before[index];
        let before = self.nav_before;
        self.nav_before = [now[0], now[1], now[2], now[3]];

        if !scene_accepts_input() {
            return;
        }
        if self.prompt_just_closed {
            self.prompt_just_closed = false;
            return;
        }
        if self.prompt.is_some() {
            return;
        }
        if just(0, &before) {
            self.navigate(true);
        }
        if just(1, &before) {
            self.navigate(false);
        }
        if just(2, &before) {
            self.open_prompt();
            return;
        }
        self.cancel_gesture(delta, just(3, &before), released(3, &before));
    }
}

/// The scene's fixed resolution: an action's key in the config's default
/// keymap, untouched by the live overrides this scene edits.
fn default_key(config: &GameConfig, action: GameAction) -> GameKey {
    config
        .defaults
        .keymap
        .binding(action)
        .expect("validated: defaults.keymap binds every action")
}

fn key_label(
    action: GameAction,
    keymap: &crate::core::input::Keymap,
    config: &GameConfig,
) -> String {
    keymap.key(action, config).name()
}
