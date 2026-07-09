use crate::core::input::Keymap;
use crate::core::note_field::NoteSpeed;
use crate::core::persist::{load_user_data, save_user_data};
use crate::core::player::{PerPlayer, PlayerId};
use crate::core::units::{Millis, Seconds};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::ops::{Index, IndexMut};
use strum::IntoEnumIterator;

/// Settings that belong to the machine rather than to either player: the
/// key bindings for both player slots and the rig's timing calibration.
/// Any mutation is automatically persisted to disk.
#[derive(Resource, Debug, Clone, PartialEq, Serialize)]
pub struct MachineSettings {
    pub keymap: Keymap,
    pub timing: TimingSettings,
}

/// Each player slot's own presentation options, indexed by player. Any
/// mutation is automatically persisted, one file per player.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct PlayerSettings(PerPlayer<PlayerOptions>);

impl Index<PlayerId> for PlayerSettings {
    type Output = PlayerOptions;

    fn index(&self, player: PlayerId) -> &PlayerOptions {
        &self.0[player]
    }
}

impl IndexMut<PlayerId> for PlayerSettings {
    fn index_mut(&mut self, player: PlayerId) -> &mut PlayerOptions {
        &mut self.0[player]
    }
}

/// One player's presentation choices for playing stepfiles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerOptions {
    /// Folder name of the note skin under `assets/note_skins`.
    pub note_skin: String,
    pub note_speed: NoteSpeed,
}

/// The synchronization model:
///
/// ```text
/// heard   = audio position - audio_latency   (what the speakers play now)
/// graded  = heard + machine_offset           (timeline inputs are graded on)
/// visible = graded - visual_delay            (timeline arrows are drawn on)
/// ```
///
/// The audio backend only reports the mixer's queue position, so the
/// latency between queue and speakers is measured on first play and stored
/// here. `machine_offset` shifts the graded timeline to compensate for the
/// rig as a whole; `visual_delay` shifts only what is drawn.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TimingSettings {
    pub machine_offset: Millis,
    pub visual_delay: Millis,
    /// `None` until measured on first play; editable afterwards.
    pub audio_latency: Option<Millis>,
}

impl TimingSettings {
    pub fn audio_latency(&self) -> Millis {
        self.audio_latency.unwrap_or(Millis(0))
    }

    /// What the speakers are playing right now, given the mixer's queue
    /// position.
    fn heard(&self, position: Seconds) -> Seconds {
        position - self.audio_latency().to_seconds()
    }

    /// The timeline inputs are graded on.
    pub fn graded(&self, position: Seconds) -> Seconds {
        self.heard(position) + self.machine_offset.to_seconds()
    }

    /// The timeline everything is drawn on.
    pub fn visible(&self, position: Seconds) -> Seconds {
        self.graded(position) - self.visual_delay.to_seconds()
    }
}

pub struct SettingsPlugin {
    /// Player options for settings files that predate the field.
    pub default_options: PlayerOptions,
    /// Timing for settings files that predate the field.
    pub default_timing: TimingSettings,
}

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(load_machine_settings(self.default_timing.clone()))
            .insert_resource(load_player_settings(self.default_options.clone()))
            .add_systems(Update, (save_machine_settings, save_player_settings));
    }
}

/// The on-disk shape: every section optional, so files written by older
/// versions still load.
#[derive(Default, Deserialize)]
#[serde(default)]
struct MachineSettingsFile {
    keymap: Keymap,
    timing: Option<TimingSettings>,
}

const MACHINE_SETTINGS_FILE: &str = "machine_settings.json";

fn player_settings_file(player: PlayerId) -> &'static str {
    match player {
        PlayerId::P1 => "p1_settings.json",
        PlayerId::P2 => "p2_settings.json",
    }
}

fn load_machine_settings(default_timing: TimingSettings) -> MachineSettings {
    let file: MachineSettingsFile = load_user_data(MACHINE_SETTINGS_FILE);
    MachineSettings {
        keymap: file.keymap,
        timing: file.timing.unwrap_or(default_timing),
    }
}

fn load_player_settings(default_options: PlayerOptions) -> PlayerSettings {
    let load = |player: PlayerId| {
        load_user_data::<Option<PlayerOptions>>(player_settings_file(player))
            .unwrap_or_else(|| default_options.clone())
    };
    PlayerSettings(PerPlayer {
        p1: load(PlayerId::P1),
        p2: load(PlayerId::P2),
    })
}

/// Persists on every edit; the initial insertion is not an edit.
fn save_machine_settings(settings: Res<MachineSettings>) {
    if settings.is_changed() && !settings.is_added() {
        save_user_data(MACHINE_SETTINGS_FILE, &*settings);
    }
}

/// Persists on every edit; the initial insertion is not an edit.
fn save_player_settings(settings: Res<PlayerSettings>) {
    if settings.is_changed() && !settings.is_added() {
        for player in PlayerId::iter() {
            save_user_data(player_settings_file(player), &settings.0[player]);
        }
    }
}
