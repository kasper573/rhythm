use crate::core::config::GameConfig;
use crate::core::player::{PerPlayer, PlayerId};
use godot::classes::{Input, InputEventKey, InputMap, Os};
use godot::global::Key;
use godot::prelude::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use strum::{EnumCount, EnumIter, IntoEnumIterator, IntoStaticStr};

/// Every key the machine responds to, as one flat list: one set of player
/// actions per player slot, plus the machine-wide tuning toggles. Menus
/// listen to P1 alone; shared spaces (the wheel, the player options modal)
/// listen to every active player.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    EnumCount,
    EnumIter,
    IntoStaticStr,
    GodotConvert,
    Var,
)]
#[godot(via = i64)]
pub enum GameAction {
    #[strum(serialize = "P1 Step left")]
    P1Left,
    #[strum(serialize = "P1 Step down")]
    P1Down,
    #[strum(serialize = "P1 Step up")]
    P1Up,
    #[strum(serialize = "P1 Step right")]
    P1Right,
    #[strum(serialize = "P1 Select")]
    P1Select,
    #[strum(serialize = "P1 Cancel")]
    P1Cancel,
    #[strum(serialize = "P2 Step left")]
    P2Left,
    #[strum(serialize = "P2 Step down")]
    P2Down,
    #[strum(serialize = "P2 Step up")]
    P2Up,
    #[strum(serialize = "P2 Step right")]
    P2Right,
    #[strum(serialize = "P2 Select")]
    P2Select,
    #[strum(serialize = "P2 Cancel")]
    P2Cancel,
    #[strum(serialize = "Toggle AutoSync")]
    ToggleAutoSync,
    #[strum(serialize = "Toggle tick audio")]
    ToggleTickAudio,
    #[strum(serialize = "Decrease audio latency")]
    DecreaseAudioLatency,
    #[strum(serialize = "Increase audio latency")]
    IncreaseAudioLatency,
    #[strum(serialize = "Decrease visual delay")]
    DecreaseVisualDelay,
    #[strum(serialize = "Increase visual delay")]
    IncreaseVisualDelay,
    #[strum(serialize = "Decrease machine offset")]
    DecreaseMachineOffset,
    #[strum(serialize = "Increase machine offset")]
    IncreaseMachineOffset,
    #[strum(serialize = "Toggle FPS")]
    ToggleFps,
}

/// A step panel's direction, in the fixed Left/Down/Up/Right column order
/// every 4-panel pad uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepDirection {
    Left,
    Down,
    Up,
    Right,
}

impl StepDirection {
    /// The direction of a pad-local column (`0..4`).
    pub fn of_column(column: usize) -> StepDirection {
        match column {
            0 => StepDirection::Left,
            1 => StepDirection::Down,
            2 => StepDirection::Up,
            _ => StepDirection::Right,
        }
    }

    /// The pad-local column of this direction — [`of_column`]'s inverse.
    pub fn column(self) -> usize {
        match self {
            StepDirection::Left => 0,
            StepDirection::Down => 1,
            StepDirection::Up => 2,
            StepDirection::Right => 3,
        }
    }
}

/// Each player's step actions in [`StepDirection`] column order — the one
/// table both directions of the step mapping read from.
const STEP_ACTIONS: PerPlayer<[GameAction; 4]> = PerPlayer {
    p1: [
        GameAction::P1Left,
        GameAction::P1Down,
        GameAction::P1Up,
        GameAction::P1Right,
    ],
    p2: [
        GameAction::P2Left,
        GameAction::P2Down,
        GameAction::P2Up,
        GameAction::P2Right,
    ],
};

impl GameAction {
    pub fn label(self) -> &'static str {
        self.into()
    }

    /// The Godot [`InputMap`] action this maps to.
    pub fn action_name(self) -> StringName {
        StringName::from(&format!("{self:?}"))
    }

    pub fn step(player: PlayerId, direction: StepDirection) -> GameAction {
        STEP_ACTIONS[player][direction.column()]
    }

    /// The `(player, direction)` a step action belongs to; `None` for
    /// everything that is not a step.
    pub fn as_step(self) -> Option<(PlayerId, StepDirection)> {
        PlayerId::iter().find_map(|player| {
            let column = STEP_ACTIONS[player]
                .iter()
                .position(|action| *action == self)?;
            Some((player, StepDirection::of_column(column)))
        })
    }

    pub fn select(player: PlayerId) -> GameAction {
        match player {
            PlayerId::P1 => GameAction::P1Select,
            PlayerId::P2 => GameAction::P2Select,
        }
    }

    pub fn cancel(player: PlayerId) -> GameAction {
        match player {
            PlayerId::P1 => GameAction::P1Cancel,
            PlayerId::P2 => GameAction::P2Cancel,
        }
    }
}

/// A physical key, named in JSON by Godot's own keycode strings
/// (`OS.get_keycode_string`), e.g. `"A"`, `"Up"`, `"Escape"`, `"F5"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GameKey(pub Key);

impl PartialOrd for GameKey {
    fn partial_cmp(&self, other: &GameKey) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GameKey {
    fn cmp(&self, other: &GameKey) -> std::cmp::Ordering {
        self.0.ord().cmp(&other.0.ord())
    }
}

impl GameKey {
    pub fn name(self) -> String {
        Os::singleton().get_keycode_string(self.0).to_string()
    }
}

impl Serialize for GameKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.name())
    }
}

impl<'de> Deserialize<'de> for GameKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<GameKey, D::Error> {
        let name = String::deserialize(deserializer)?;
        let key = Os::singleton().find_keycode_from_string(&name);
        if key == Key::NONE {
            return Err(serde::de::Error::custom(format!("unknown key {name:?}")));
        }
        Ok(GameKey(key))
    }
}

/// A set of key bindings. The machine settings hold the players'
/// overrides; actions without one resolve through the config's
/// `defaults.keymap`, which binds everything.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Keymap(BTreeMap<GameAction, GameKey>);

impl Keymap {
    pub fn key(&self, action: GameAction, config: &GameConfig) -> GameKey {
        self.binding(action)
            .or_else(|| config.defaults.keymap.binding(action))
            .expect("validated: defaults.keymap binds every action")
    }

    pub fn binding(&self, action: GameAction) -> Option<GameKey> {
        self.0.get(&action).copied()
    }

    pub fn set(&mut self, action: GameAction, key: GameKey) {
        self.0.insert(action, key);
    }

    pub fn reset(&mut self, action: GameAction) {
        self.0.remove(&action);
    }

    /// Installs the resolved bindings as Godot [`InputMap`] actions, so
    /// game code reads them through [`Actions`]. Call after any change.
    pub fn apply_input_map(&self, config: &GameConfig) {
        let mut map = InputMap::singleton();
        for action in GameAction::iter() {
            let name = action.action_name();
            if map.has_action(&name) {
                map.action_erase_events(&name);
            } else {
                map.add_action(&name);
            }
            let mut event = InputEventKey::new_gd();
            event.set_physical_keycode(self.key(action, config).0);
            map.action_add_event(&name, &event);
        }
    }
}

/// The one way game code polls the rebindable actions, backed by Godot's
/// [`InputMap`] state that [`Keymap::apply_input_map`] installs.
pub struct Actions;

impl Actions {
    pub fn just_pressed(action: GameAction) -> bool {
        Input::singleton().is_action_just_pressed(&action.action_name())
    }

    pub fn pressed(action: GameAction) -> bool {
        Input::singleton().is_action_pressed(&action.action_name())
    }

    pub fn just_released(action: GameAction) -> bool {
        Input::singleton().is_action_just_released(&action.action_name())
    }

    /// Whether any of the given players just pressed their variant of an
    /// action — the shared-space check for anything both players may drive.
    pub fn any_just_pressed(players: &[PlayerId], action: fn(PlayerId) -> GameAction) -> bool {
        players
            .iter()
            .any(|player| Actions::just_pressed(action(*player)))
    }
}

pub fn shift_held() -> bool {
    Input::singleton().is_physical_key_pressed(Key::SHIFT)
}

/// Touch controls, synthesized as the actions the game already
/// understands. Every touch acts independently: a swipe presses its
/// direction's step the moment it crosses the threshold and releases it
/// when the finger lifts — so simultaneous swipes play jumps, and a swipe
/// kept down sustains holds. A short stationary tap is ¤Select¤; exactly
/// two touches held stationary for a second are ¤Cancel¤.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct TouchSteps {
    touches: std::collections::HashMap<i64, TrackedTouch>,
    cancel_hold: f64,
    /// Synthesized taps stay pressed briefly so action edges register.
    releases: Vec<(GameAction, f64)>,
    base: Base<Node>,
}

struct TrackedTouch {
    start: Vector2,
    position: Vector2,
    arrow: Option<GameAction>,
    consumed: bool,
}

const SWIPE_MIN: f32 = 30.0;
const CANCEL_HOLD_SECONDS: f64 = 1.0;
const TAP_PULSE_SECONDS: f64 = 0.06;

impl TouchSteps {
    fn press(&mut self, action: GameAction) {
        Input::singleton().action_press(&action.action_name());
    }

    fn release(&mut self, action: GameAction) {
        Input::singleton().action_release(&action.action_name());
    }

    fn pulse(&mut self, action: GameAction) {
        self.press(action);
        self.releases.push((action, TAP_PULSE_SECONDS));
    }

    fn swipe_arrow(touch: &TrackedTouch) -> Option<StepDirection> {
        let delta = touch.position - touch.start;
        if delta.x.abs().max(delta.y.abs()) < SWIPE_MIN {
            return None;
        }
        Some(if delta.x.abs() > delta.y.abs() {
            if delta.x > 0.0 {
                StepDirection::Right
            } else {
                StepDirection::Left
            }
        } else if delta.y > 0.0 {
            StepDirection::Down
        } else {
            StepDirection::Up
        })
    }
}

#[godot_api]
impl godot::classes::INode for TouchSteps {
    fn init(base: Base<Node>) -> TouchSteps {
        TouchSteps {
            touches: std::collections::HashMap::new(),
            cancel_hold: 0.0,
            releases: Vec::new(),
            base,
        }
    }

    fn input(&mut self, event: Gd<godot::classes::InputEvent>) {
        if let Ok(touch) = event
            .clone()
            .try_cast::<godot::classes::InputEventScreenTouch>()
        {
            let index = touch.get_index() as i64;
            if touch.is_pressed() {
                self.touches.insert(
                    index,
                    TrackedTouch {
                        start: touch.get_position(),
                        position: touch.get_position(),
                        arrow: None,
                        consumed: false,
                    },
                );
            } else if let Some(tracked) = self.touches.remove(&index) {
                match tracked.arrow {
                    Some(action) => self.release(action),
                    None if !tracked.consumed => self.pulse(GameAction::select(PlayerId::P1)),
                    None => {}
                }
            }
            return;
        }
        let Ok(drag) = event.try_cast::<godot::classes::InputEventScreenDrag>() else {
            return;
        };
        let index = drag.get_index() as i64;
        let position = drag.get_position();
        let Some(tracked) = self.touches.get_mut(&index) else {
            return;
        };
        tracked.position = position;
        if tracked.arrow.is_some() {
            return;
        }
        if let Some(direction) = TouchSteps::swipe_arrow(tracked) {
            let action = GameAction::step(PlayerId::P1, direction);
            self.touches
                .get_mut(&index)
                .expect("touch tracked just above")
                .arrow = Some(action);
            self.press(action);
        }
    }

    fn process(&mut self, delta: f64) {
        let mut due = Vec::new();
        self.releases.retain_mut(|(action, remaining)| {
            *remaining -= delta;
            if *remaining <= 0.0 {
                due.push(*action);
                return false;
            }
            true
        });
        for action in due {
            self.release(action);
        }

        // Exactly two touches held stationary for a second are ¤Cancel¤.
        let stationary = self
            .touches
            .values()
            .filter(|touch| touch.arrow.is_none() && !touch.consumed)
            .count();
        if stationary == 2 && self.touches.len() == 2 {
            self.cancel_hold += delta;
            if self.cancel_hold >= CANCEL_HOLD_SECONDS {
                self.cancel_hold = 0.0;
                for tracked in self.touches.values_mut() {
                    tracked.consumed = true;
                }
                self.pulse(GameAction::cancel(PlayerId::P1));
            }
        } else {
            self.cancel_hold = 0.0;
        }
    }
}
