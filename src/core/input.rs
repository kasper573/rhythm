use crate::core::config::GameConfig;
use crate::core::settings::Settings;
use crate::core::units::Seconds;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use strum::{EnumCount, EnumIter, IntoStaticStr};

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
    Select,
    Cancel,
    Reset,
    #[strum(serialize = "Step left")]
    Left,
    #[strum(serialize = "Step down")]
    Down,
    #[strum(serialize = "Step up")]
    Up,
    #[strum(serialize = "Step right")]
    Right,
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

impl GameAction {
    pub fn label(self) -> &'static str {
        self.into()
    }
}

/// The player's key overrides; actions without one use the config's default.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Keymap(BTreeMap<GameAction, KeyCode>);

impl Keymap {
    pub fn key(&self, action: GameAction, config: &GameConfig) -> KeyCode {
        self.0
            .get(&action)
            .copied()
            .unwrap_or(config.default_keymap[&action])
    }

    /// For systems that need mutable [`Settings`](crate::core::settings::Settings)
    /// access and therefore cannot use the [`Actions`] param.
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
    settings: Res<'w, Settings>,
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

    fn key(&self, action: GameAction) -> KeyCode {
        self.settings.keymap.key(action, &self.config)
    }
}

pub fn shift_held(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight)
}

/// A press of ¤Next¤/¤Previous¤/¤Left¤/¤Right¤, re-fired while held so
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

fn emit_nav_pulses(
    actions: Actions,
    time: Res<Time>,
    mut held: Local<[Seconds; 4]>,
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
