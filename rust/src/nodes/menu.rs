use crate::core::font::label;
use crate::core::input::{Actions, GameAction, StepDirection};
use crate::core::player::PlayerId;
use crate::core::screen::{ACTIVE_COLOR, INACTIVE_COLOR, TITLE_COLOR};
use crate::core::sfx::Sfx;
use crate::core::units::Seconds;
use godot::classes::control::LayoutPreset;
use godot::classes::{CenterContainer, Control, Engine, IControl, Label, VBoxContainer};
use godot::prelude::*;

pub struct MenuOptions {
    pub title: String,
    pub items: Vec<String>,
}

/// A full-screen titled menu, driving itself from the shared [`NavInput`]:
/// P1's ¤Up¤/¤Down¤ step pulses move the highlight and their ¤Select¤
/// fires the `selected` signal with the active item's index. Owners that
/// drive the highlight and selection themselves — a keymap editor must
/// stay operable however broken the stored bindings are — flip
/// [`set_owner_driven`](Menu::set_owner_driven) and only the highlight runs.
#[derive(GodotClass)]
#[class(base=Control)]
pub struct Menu {
    active: usize,
    len: usize,
    owner_driven: bool,
    items: Vec<Gd<Label>>,
    base: Base<Control>,
}

#[godot_api]
impl Menu {
    /// The active item was chosen.
    #[signal]
    pub fn selected(index: i64);

    pub fn instantiate(opt: MenuOptions) -> Gd<Menu> {
        let len = opt.items.len();
        let mut menu = Menu::new_alloc();
        menu.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);

        let mut center = CenterContainer::new_alloc();
        center.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        let mut column = VBoxContainer::new_alloc();
        column.set_alignment(godot::classes::box_container::AlignmentMode::CENTER);
        column.add_theme_constant_override("separation", 12);

        let mut title = label(&opt.title, 52.0, TITLE_COLOR);
        title.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
        column.add_child(&title);
        let mut spacer = Control::new_alloc();
        spacer.set_custom_minimum_size(Vector2::new(0.0, 32.0));
        column.add_child(&spacer);

        let mut items = Vec::new();
        for (index, text) in opt.items.iter().enumerate() {
            let color = if index == 0 {
                ACTIVE_COLOR
            } else {
                INACTIVE_COLOR
            };
            let mut item = label(text, 34.0, color);
            item.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
            column.add_child(&item);
            items.push(item);
        }
        center.add_child(&column);
        menu.add_child(&center);

        let mut bound = menu.bind_mut();
        bound.len = len;
        bound.items = items;
        drop(bound);
        menu
    }

    /// The highlighted item.
    pub fn active(&self) -> usize {
        self.active
    }

    /// Moves the highlight directly — the owner-driven path.
    pub fn set_active(&mut self, active: usize) {
        self.active = active;
        self.refresh_highlight();
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn set_owner_driven(&mut self) {
        self.owner_driven = true;
    }

    fn step(&mut self, back: bool) {
        if self.len == 0 {
            return;
        }
        self.active = if back {
            (self.active + self.len - 1) % self.len
        } else {
            (self.active + 1) % self.len
        };
        self.refresh_highlight();
        Sfx::Navigate.play();
    }

    fn refresh_highlight(&mut self) {
        for (index, item) in self.items.iter_mut().enumerate() {
            let color = if index == self.active {
                ACTIVE_COLOR
            } else {
                INACTIVE_COLOR
            };
            item.add_theme_color_override("font_color", color);
        }
    }
}

#[godot_api]
impl IControl for Menu {
    fn init(base: Base<Control>) -> Menu {
        Menu {
            active: 0,
            len: 0,
            owner_driven: false,
            items: Vec::new(),
            base,
        }
    }

    fn process(&mut self, _delta: f64) {
        if self.owner_driven || !NavInput::active() {
            return;
        }
        for pulse in NavInput::pulses() {
            let Some((PlayerId::P1, direction)) = pulse.as_step() else {
                continue;
            };
            match direction {
                StepDirection::Up => self.step(true),
                StepDirection::Down => self.step(false),
                _ => {}
            }
        }
        if self.len > 0 && Actions::just_pressed(GameAction::select(PlayerId::P1)) {
            let index = self.active as i64;
            Sfx::Select.play();
            self.signals().selected().emit(index);
        }
    }
}

const REPEAT_DELAY: Seconds = Seconds(0.4);
const REPEAT_INTERVAL: Seconds = Seconds(0.09);

/// Every action lists and panels scroll by: each player's step panel.
const PULSE_ACTIONS: [GameAction; 8] = [
    GameAction::P1Left,
    GameAction::P1Down,
    GameAction::P1Up,
    GameAction::P1Right,
    GameAction::P2Left,
    GameAction::P2Down,
    GameAction::P2Up,
    GameAction::P2Right,
];

/// The shared navigation vocabulary: a press of a step action, re-fired
/// while held so lists scroll comfortably. Menus and menu-like scenes (the
/// wheel, options panels) consume [`pulses`](NavInput::pulses) instead of
/// raw key state. Processes before every scene (it sits first under the
/// root), so a frame's pulses are ready when consumers run; the root
/// suspends it while a scene transition runs.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct NavInput {
    held: [Seconds; PULSE_ACTIONS.len()],
    pulses: Vec<GameAction>,
    suspended: bool,
    base: Base<Node>,
}

#[godot_api]
impl NavInput {
    pub fn singleton() -> Gd<NavInput> {
        Engine::singleton()
            .get_singleton("NavInput")
            .expect("NavInput singleton is registered at boot")
            .cast()
    }

    /// This frame's pulses.
    pub fn pulses() -> Vec<GameAction> {
        NavInput::singleton().bind().pulses.clone()
    }

    /// Whether navigation input is live (no scene transition running).
    pub fn active() -> bool {
        !NavInput::singleton().bind().suspended
    }

    /// Suspends or resumes pulse emission; suspension also drops buffered
    /// pulses so a resumed consumer never replays stale ones.
    pub fn set_suspended(&mut self, suspended: bool) {
        self.suspended = suspended;
        if suspended {
            self.pulses.clear();
        }
    }

    /// Clears this frame's pulses — for focus handoffs where the pulse
    /// must not reach the mode that did not consume it.
    pub fn clear(&mut self) {
        self.pulses.clear();
    }
}

#[godot_api]
impl INode for NavInput {
    fn init(base: Base<Node>) -> NavInput {
        NavInput {
            held: [Seconds::ZERO; PULSE_ACTIONS.len()],
            pulses: Vec::new(),
            suspended: false,
            base,
        }
    }

    fn process(&mut self, delta: f64) {
        self.pulses.clear();
        if self.suspended {
            return;
        }
        for (slot, action) in PULSE_ACTIONS.into_iter().enumerate() {
            if Actions::just_pressed(action) {
                self.held[slot] = Seconds::ZERO;
                self.pulses.push(action);
            } else if Actions::pressed(action) {
                let before = self.held[slot];
                self.held[slot] += Seconds(delta);
                let repeats_before = ((before - REPEAT_DELAY) / REPEAT_INTERVAL).floor();
                let repeats_after = ((self.held[slot] - REPEAT_DELAY) / REPEAT_INTERVAL).floor();
                if self.held[slot] >= REPEAT_DELAY && repeats_after > repeats_before {
                    self.pulses.push(action);
                }
            } else {
                self.held[slot] = Seconds::ZERO;
            }
        }
    }
}
