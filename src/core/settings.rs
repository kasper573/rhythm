use crate::core::config::StepfileOptions;
use crate::core::input::Keymap;
use crate::core::persist::{load_user_data, save_user_data};
use crate::core::units::Millis;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Any mutation is automatically persisted to disk.
#[derive(Resource, Debug, Clone, PartialEq, Serialize)]
pub struct Settings {
    pub keymap: Keymap,
    pub timing: TimingSettings,
    pub stepfile: StepfileOptions,
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
}

pub struct SettingsPlugin {
    /// Stepfile options for settings files that predate the field.
    pub default_stepfile: StepfileOptions,
}

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(load_settings(self.default_stepfile.clone()))
            .add_systems(Update, save_settings);
    }
}

/// The on-disk shape: every section optional, so files written by older
/// versions still load.
#[derive(Default, Deserialize)]
#[serde(default)]
struct SettingsFile {
    keymap: Keymap,
    timing: TimingSettings,
    stepfile: Option<StepfileOptions>,
}

const SETTINGS_FILE: &str = "settings.json";

fn load_settings(default_stepfile: StepfileOptions) -> Settings {
    let file: SettingsFile = load_user_data(SETTINGS_FILE);
    Settings {
        keymap: file.keymap,
        timing: file.timing,
        stepfile: file.stepfile.unwrap_or(default_stepfile),
    }
}

/// Persists on every edit; the initial insertion is not an edit.
fn save_settings(settings: Res<Settings>) {
    if settings.is_changed() && !settings.is_added() {
        save_user_data(SETTINGS_FILE, &*settings);
    }
}
