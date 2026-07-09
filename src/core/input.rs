use crate::core::config::GameConfig;
use crate::core::player::PlayerId;
use crate::core::settings::MachineSettings;
use crate::core::units::Seconds;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use strum::{EnumCount, EnumIter, IntoEnumIterator, IntoStaticStr};

/// Every key the machine responds to, as one flat list: machine-wide
/// actions plus one set of player actions per player slot. Menus listen to
/// the machine-wide navigation and to P1 alone; shared spaces (the wheel,
/// the player options modal) listen to every active player.
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
)]
pub enum GameAction {
    Next,
    Previous,
    Reset,
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
}

/// Each player's step actions in [`StepDirection`] column order — the one
/// table both directions of the step mapping read from.
const STEP_ACTIONS: [[GameAction; 4]; 2] = [
    [
        GameAction::P1Left,
        GameAction::P1Down,
        GameAction::P1Up,
        GameAction::P1Right,
    ],
    [
        GameAction::P2Left,
        GameAction::P2Down,
        GameAction::P2Up,
        GameAction::P2Right,
    ],
];

impl GameAction {
    pub fn label(self) -> &'static str {
        self.into()
    }

    pub fn step(player: PlayerId, direction: StepDirection) -> GameAction {
        STEP_ACTIONS[player as usize][direction as usize]
    }

    /// The `(player, direction)` a step action belongs to; `None` for
    /// everything that is not a step.
    pub fn as_step(self) -> Option<(PlayerId, StepDirection)> {
        PlayerId::iter().find_map(|player| {
            let column = STEP_ACTIONS[player as usize]
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

/// A set of key bindings. The machine settings hold the players'
/// overrides; actions without one resolve through the config's
/// `defaults.keymap`, which binds everything.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Keymap(BTreeMap<GameAction, KeyCode>);

impl Keymap {
    pub fn key(&self, action: GameAction, config: &GameConfig) -> KeyCode {
        self.binding(action)
            .or_else(|| config.defaults.keymap.binding(action))
            .expect("validated: defaults.keymap binds every action")
    }

    pub fn binding(&self, action: GameAction) -> Option<KeyCode> {
        self.0.get(&action).copied()
    }

    /// For systems that need mutable [`MachineSettings`] access and
    /// therefore cannot use the [`Actions`] param.
    pub fn just_pressed(
        &self,
        keys: &ButtonInput<KeyCode>,
        action: GameAction,
        config: &GameConfig,
    ) -> bool {
        keys.just_pressed(self.key(action, config))
    }

    pub fn set(&mut self, action: GameAction, key: KeyCode) {
        self.0.insert(action, key);
    }

    pub fn reset(&mut self, action: GameAction) {
        self.0.remove(&action);
    }
}

#[derive(SystemParam)]
pub struct Actions<'w> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    settings: Res<'w, MachineSettings>,
    config: Res<'w, GameConfig>,
}

impl Actions<'_> {
    pub fn just_pressed(&self, action: GameAction) -> bool {
        self.keys.just_pressed(self.key(action))
    }

    pub fn pressed(&self, action: GameAction) -> bool {
        self.keys.pressed(self.key(action))
    }

    pub fn just_released(&self, action: GameAction) -> bool {
        self.keys.just_released(self.key(action))
    }

    /// Whether any of the given players just pressed their variant of an
    /// action — the shared-space check for anything both players may drive.
    pub fn any_just_pressed(
        &self,
        players: &[PlayerId],
        action: fn(PlayerId) -> GameAction,
    ) -> bool {
        players
            .iter()
            .any(|player| self.just_pressed(action(*player)))
    }

    fn key(&self, action: GameAction) -> KeyCode {
        self.settings.keymap.key(action, &self.config)
    }
}

pub fn shift_held(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight)
}

/// A press of a menu or step navigation action, re-fired while held so
/// lists scroll comfortably.
#[derive(Message)]
pub struct NavPulse {
    pub action: GameAction,
}

/// The pulse emitter's slot in `PreUpdate`: synthetic key state (the
/// bench scenarios) must land after bevy's input update and before this
/// set to register on the frame it was written.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NavPulseSystems;

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<NavPulse>().add_systems(
            PreUpdate,
            emit_nav_pulses
                .in_set(NavPulseSystems)
                .after(bevy::input::InputSystems),
        );
    }
}

const REPEAT_DELAY: Seconds = Seconds(0.4);
const REPEAT_INTERVAL: Seconds = Seconds(0.09);

/// Every action lists and panels scroll by: the machine-wide menu pair
/// plus each player's step panel.
const PULSE_ACTIONS: [GameAction; 10] = [
    GameAction::Next,
    GameAction::Previous,
    GameAction::P1Left,
    GameAction::P1Down,
    GameAction::P1Up,
    GameAction::P1Right,
    GameAction::P2Left,
    GameAction::P2Down,
    GameAction::P2Up,
    GameAction::P2Right,
];

fn emit_nav_pulses(
    actions: Actions,
    time: Res<Time>,
    mut held: Local<[Seconds; PULSE_ACTIONS.len()]>,
    mut pulses: MessageWriter<NavPulse>,
) {
    for (slot, action) in PULSE_ACTIONS.into_iter().enumerate() {
        if actions.just_pressed(action) {
            held[slot] = Seconds::ZERO;
            pulses.write(NavPulse { action });
        } else if actions.pressed(action) {
            let before = held[slot];
            held[slot] += Seconds(time.delta_secs_f64());
            let repeats_before = ((before - REPEAT_DELAY) / REPEAT_INTERVAL).floor();
            let repeats_after = ((held[slot] - REPEAT_DELAY) / REPEAT_INTERVAL).floor();
            if held[slot] >= REPEAT_DELAY && repeats_after > repeats_before {
                pulses.write(NavPulse { action });
            }
        } else {
            held[slot] = Seconds::ZERO;
        }
    }
}
