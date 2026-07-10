use crate::core::config::{GameConfig, SettingsDefaults};
use crate::core::input::Keymap;
use crate::core::persist::{load_user_data, save_user_data};
use crate::core::player::{PerPlayer, PlayerId};
use crate::core::units::{Millis, Percent, Seconds};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::ops::{Index, IndexMut};
use strum::{EnumIter, EnumString, IntoEnumIterator, IntoStaticStr};

/// Settings that belong to the machine rather than to either player: the
/// key bindings for both player slots, the rig's timing calibration, and
/// its playback volumes. Any mutation is automatically persisted to disk.
#[derive(Resource, Debug, Clone, PartialEq, Serialize)]
pub struct MachineSettings {
    pub keymap: Keymap,
    pub timing: TimingSettings,
    pub volume: VolumeSettings,
}

/// Playback volumes, each `0..=1`: `master` scales everything, the others
/// their own bus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VolumeSettings {
    pub master: f32,
    pub sfx: f32,
    pub music: f32,
}

impl VolumeSettings {
    /// The gain music playback runs at.
    pub fn music_gain(&self) -> f32 {
        self.master * self.music
    }

    /// The gain sound effects (and the tick track) run at.
    pub fn sfx_gain(&self) -> f32 {
        self.master * self.sfx
    }
}

/// Each player slot's own presentation options, indexed by player. Any
/// mutation is automatically persisted, one file per player.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct PlayerSettings(PerPlayer<PlayerOptions>);

impl PlayerSettings {
    /// Both players on one set of options — for tools that run without
    /// [`SettingsPlugin`] (nothing is persisted).
    pub fn uniform(options: PlayerOptions) -> PlayerSettings {
        PlayerSettings(PerPlayer {
            p1: options.clone(),
            p2: options,
        })
    }
}

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
    pub perspective: Perspective,
    pub grade_layer: GradeLayer,
    /// Grade text height as a percentage down the screen: 0 hugs the top
    /// edge, 100 the bottom edge, ignoring the stage's edge padding.
    pub grade_position: Percent,
}

/// Whether the grade text pops out behind the arrows or in front of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, EnumIter, IntoStaticStr)]
pub enum GradeLayer {
    Behind,
    #[strum(serialize = "In front")]
    InFront,
}

/// How fast notes scroll for one player.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum NoteSpeed {
    /// A constant rate regardless of the chart's tempo, expressed as the
    /// scroll BPM at which [`NoteSpeed::Dynamic`] would move equally fast.
    Constant(f32),
    /// Spacing follows the chart's beats — one arrow height per beat at
    /// multiplier 1 — so BPM changes stretch the scroll and stops freeze it.
    Dynamic(f32),
}

impl NoteSpeed {
    pub fn value(self) -> f32 {
        match self {
            NoteSpeed::Constant(value) | NoteSpeed::Dynamic(value) => value,
        }
    }
}

/// Where a player's lane camera watches their arrows from. The receptor
/// row stays put on screen; the rest of the lane foreshortens around it.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, EnumIter, EnumString, IntoStaticStr,
)]
pub enum Perspective {
    /// Head on: no perspective.
    None,
    /// From above: notes rise out of the distance below.
    Above,
    /// From below: the lane recedes upward.
    Below,
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// Loads the settings on startup, backfilling anything missing — whole
/// files or single fields — from the config's [`SettingsDefaults`]: the
/// code holds no default values of its own. Requires [`GameConfig`] to
/// already be inserted.
pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        let defaults = app.world().resource::<GameConfig>().defaults.clone();
        app.insert_resource(load_machine_settings(&defaults))
            .insert_resource(load_player_settings(&defaults))
            .add_systems(Update, (save_machine_settings, save_player_settings));
    }
}

/// The on-disk shape: every field optional, so files written by older
/// versions still load — absent fields resolve to the config defaults.
#[derive(Default, Deserialize)]
#[serde(default)]
struct MachineSettingsFile {
    keymap: Option<Keymap>,
    timing: TimingSettingsFile,
    volume: VolumeSettingsFile,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct TimingSettingsFile {
    machine_offset: Option<Millis>,
    visual_delay: Option<Millis>,
    audio_latency: Option<Millis>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct VolumeSettingsFile {
    master: Option<f32>,
    sfx: Option<f32>,
    music: Option<f32>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct PlayerOptionsFile {
    note_skin: Option<String>,
    note_speed: Option<NoteSpeed>,
    perspective: Option<Perspective>,
    grade_layer: Option<GradeLayer>,
    grade_position: Option<Percent>,
}

const MACHINE_SETTINGS_FILE: &str = "machine_settings.json";

fn player_settings_file(player: PlayerId) -> &'static str {
    match player {
        PlayerId::P1 => "p1_settings.json",
        PlayerId::P2 => "p2_settings.json",
    }
}

fn load_machine_settings(defaults: &SettingsDefaults) -> MachineSettings {
    let file: MachineSettingsFile = load_user_data(MACHINE_SETTINGS_FILE);
    MachineSettings {
        // The keymap holds overrides on top of `defaults.keymap`; a fresh
        // install simply has none.
        keymap: file.keymap.unwrap_or_default(),
        timing: TimingSettings {
            machine_offset: file
                .timing
                .machine_offset
                .unwrap_or(defaults.timing_options.machine_offset),
            visual_delay: file
                .timing
                .visual_delay
                .unwrap_or(defaults.timing_options.visual_delay),
            audio_latency: file
                .timing
                .audio_latency
                .or(defaults.timing_options.audio_latency),
        },
        volume: VolumeSettings {
            master: file.volume.master.unwrap_or(defaults.volume_options.master),
            sfx: file.volume.sfx.unwrap_or(defaults.volume_options.sfx),
            music: file.volume.music.unwrap_or(defaults.volume_options.music),
        },
    }
}

fn load_player_settings(defaults: &SettingsDefaults) -> PlayerSettings {
    let load = |player: PlayerId| {
        let file: PlayerOptionsFile = load_user_data(player_settings_file(player));
        PlayerOptions {
            note_skin: file
                .note_skin
                .unwrap_or_else(|| defaults.player_options.note_skin.clone()),
            note_speed: file
                .note_speed
                .unwrap_or(defaults.player_options.note_speed),
            perspective: file
                .perspective
                .unwrap_or(defaults.player_options.perspective),
            grade_layer: file
                .grade_layer
                .unwrap_or(defaults.player_options.grade_layer),
            grade_position: file
                .grade_position
                .unwrap_or(defaults.player_options.grade_position),
        }
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
