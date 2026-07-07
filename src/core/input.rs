use crate::core::settings::Settings;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Every remappable action in the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum GameAction {
    Next,
    Previous,
    Select,
    Cancel,
    Reset,
    Left,
    Down,
    Up,
    Right,
    ToggleAutoSync,
    ToggleTickAudio,
    DecreaseAudioLatency,
    IncreaseAudioLatency,
    DecreaseVisualDelay,
    IncreaseVisualDelay,
    DecreaseMachineOffset,
    IncreaseMachineOffset,
}

impl GameAction {
    pub const ALL: [GameAction; 17] = [
        GameAction::Next,
        GameAction::Previous,
        GameAction::Select,
        GameAction::Cancel,
        GameAction::Reset,
        GameAction::Left,
        GameAction::Down,
        GameAction::Up,
        GameAction::Right,
        GameAction::ToggleAutoSync,
        GameAction::ToggleTickAudio,
        GameAction::DecreaseAudioLatency,
        GameAction::IncreaseAudioLatency,
        GameAction::DecreaseVisualDelay,
        GameAction::IncreaseVisualDelay,
        GameAction::DecreaseMachineOffset,
        GameAction::IncreaseMachineOffset,
    ];

    pub fn label(self) -> &'static str {
        match self {
            GameAction::Next => "Next",
            GameAction::Previous => "Previous",
            GameAction::Select => "Select",
            GameAction::Cancel => "Cancel",
            GameAction::Reset => "Reset",
            GameAction::Left => "Step left",
            GameAction::Down => "Step down",
            GameAction::Up => "Step up",
            GameAction::Right => "Step right",
            GameAction::ToggleAutoSync => "Toggle AutoSync",
            GameAction::ToggleTickAudio => "Toggle tick audio",
            GameAction::DecreaseAudioLatency => "Decrease audio latency",
            GameAction::IncreaseAudioLatency => "Increase audio latency",
            GameAction::DecreaseVisualDelay => "Decrease visual delay",
            GameAction::IncreaseVisualDelay => "Increase visual delay",
            GameAction::DecreaseMachineOffset => "Decrease machine offset",
            GameAction::IncreaseMachineOffset => "Increase machine offset",
        }
    }

    pub fn default_key(self) -> KeyCode {
        match self {
            GameAction::Next => KeyCode::ArrowDown,
            GameAction::Previous => KeyCode::ArrowUp,
            GameAction::Select => KeyCode::Enter,
            GameAction::Cancel => KeyCode::Escape,
            GameAction::Reset => KeyCode::Delete,
            GameAction::Left => KeyCode::ArrowLeft,
            GameAction::Down => KeyCode::ArrowDown,
            GameAction::Up => KeyCode::ArrowUp,
            GameAction::Right => KeyCode::ArrowRight,
            GameAction::ToggleAutoSync => KeyCode::F5,
            GameAction::ToggleTickAudio => KeyCode::F6,
            GameAction::DecreaseAudioLatency => KeyCode::F7,
            GameAction::IncreaseAudioLatency => KeyCode::F8,
            GameAction::DecreaseVisualDelay => KeyCode::F9,
            GameAction::IncreaseVisualDelay => KeyCode::F10,
            GameAction::DecreaseMachineOffset => KeyCode::F11,
            GameAction::IncreaseMachineOffset => KeyCode::F12,
        }
    }
}

/// Maps actions to physical keys. Missing entries fall back to the default.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Keymap(BTreeMap<GameAction, KeyCode>);

impl Keymap {
    pub fn key(&self, action: GameAction) -> KeyCode {
        self.0
            .get(&action)
            .copied()
            .unwrap_or_else(|| action.default_key())
    }

    /// For systems that need mutable [`Settings`](crate::core::settings::Settings)
    /// access and therefore cannot use the [`Actions`] param.
    pub fn just_pressed(&self, keys: &ButtonInput<KeyCode>, action: GameAction) -> bool {
        keys.just_pressed(self.key(action))
    }

    pub fn set(&mut self, action: GameAction, key: KeyCode) {
        self.0.insert(action, key);
    }

    pub fn reset(&mut self, action: GameAction) {
        self.0.remove(&action);
    }
}

/// Convenient action-level view over the raw keyboard input.
#[derive(SystemParam)]
pub struct Actions<'w> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    settings: Res<'w, Settings>,
}

impl Actions<'_> {
    pub fn just_pressed(&self, action: GameAction) -> bool {
        self.keys.just_pressed(self.settings.keymap.key(action))
    }

    pub fn pressed(&self, action: GameAction) -> bool {
        self.keys.pressed(self.settings.keymap.key(action))
    }

    pub fn just_released(&self, action: GameAction) -> bool {
        self.keys.just_released(self.settings.keymap.key(action))
    }

    pub fn shift_held(&self) -> bool {
        shift_held(&self.keys)
    }
}

pub fn shift_held(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight)
}

/// Fired when a navigation action (¤Next¤/¤Previous¤/¤Left¤/¤Right¤) is
/// pressed, and repeatedly while held, so lists scroll comfortably. Menus
/// and the stepfile wheel consume the actions they care about instead of
/// polling the keyboard.
#[derive(Message)]
pub struct NavPulse {
    pub action: GameAction,
}

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<NavPulse>()
            .add_systems(PreUpdate, emit_nav_pulses.after(bevy::input::InputSystems));
    }
}

const REPEAT_DELAY_SECONDS: f32 = 0.4;
const REPEAT_INTERVAL_SECONDS: f32 = 0.09;

fn emit_nav_pulses(
    actions: Actions,
    time: Res<Time>,
    mut held: Local<[f32; 4]>,
    mut pulses: MessageWriter<NavPulse>,
) {
    for (slot, action) in [
        GameAction::Next,
        GameAction::Previous,
        GameAction::Left,
        GameAction::Right,
    ]
    .into_iter()
    .enumerate()
    {
        if actions.just_pressed(action) {
            held[slot] = 0.0;
            pulses.write(NavPulse { action });
        } else if actions.pressed(action) {
            let before = held[slot];
            held[slot] += time.delta_secs();
            let repeats_before =
                ((before - REPEAT_DELAY_SECONDS) / REPEAT_INTERVAL_SECONDS).floor();
            let repeats_after =
                ((held[slot] - REPEAT_DELAY_SECONDS) / REPEAT_INTERVAL_SECONDS).floor();
            if held[slot] >= REPEAT_DELAY_SECONDS && repeats_after > repeats_before {
                pulses.write(NavPulse { action });
            }
        } else {
            held[slot] = 0.0;
        }
    }
}
