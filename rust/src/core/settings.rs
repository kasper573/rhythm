use crate::core::audio;
use crate::core::config::{SettingsDefaults, config};
use crate::core::input::Keymap;
use crate::core::persist::{load_user_data, save_user_data};
use crate::core::player::{PerPlayer, PlayerId};
use crate::core::units::{Millis, Percent, Seconds};
use godot::classes::Engine;
use godot::prelude::*;
use serde::{Deserialize, Serialize};
use std::ops::{Index, IndexMut};
use strum::{EnumIter, EnumString, IntoEnumIterator, IntoStaticStr};

/// The user settings singleton: the machine's own settings (keymap,
/// timing calibration, volumes) and each player slot's presentation
/// options. Any edit is applied on the spot — input map, audio buses —
/// persisted at the end of the frame, and visible to pollers through the
/// bumped [`revision`](Settings::revision).
#[derive(GodotClass)]
#[class(base=Node)]
pub struct Settings {
    machine: MachineSettings,
    players: PlayerSettings,
    revision: u64,
    dirty_machine: bool,
    dirty_players: bool,
    base: Base<Node>,
}

#[godot_api]
impl Settings {
    pub fn singleton() -> Gd<Settings> {
        Engine::singleton()
            .get_singleton("Settings")
            .expect("Settings singleton is registered at boot")
            .cast()
    }

    pub fn machine(&self) -> &MachineSettings {
        &self.machine
    }

    pub fn player(&self, player: PlayerId) -> &PlayerOptions {
        &self.players[player]
    }

    pub fn players(&self) -> &PlayerSettings {
        &self.players
    }

    /// Bumped by every edit; consumers poll it to react to changes.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn edit_machine(&mut self, edit: impl FnOnce(&mut MachineSettings)) {
        edit(&mut self.machine);
        self.revision += 1;
        self.dirty_machine = true;
        self.apply_machine();
    }

    pub fn edit_player(&mut self, player: PlayerId, edit: impl FnOnce(&mut PlayerOptions)) {
        edit(&mut self.players[player]);
        self.revision += 1;
        self.dirty_players = true;
    }

    fn apply_machine(&self) {
        self.machine.keymap.apply_input_map(config());
        audio::apply_volumes(&self.machine.volume);
    }
}

#[godot_api]
impl INode for Settings {
    /// Construction stays empty — the editor instantiates registered
    /// classes while scanning, where no game state exists. The real load
    /// happens on entering the booted game's tree.
    fn init(base: Base<Node>) -> Settings {
        Settings {
            machine: MachineSettings {
                keymap: Keymap::default(),
                timing: TimingSettings {
                    machine_offset: Millis(0),
                    visual_delay: Millis(0),
                    audio_latency: None,
                },
                volume: VolumeSettings {
                    master: 0.0,
                    sfx: 0.0,
                    music: 0.0,
                },
            },
            players: PlayerSettings::uniform(PlayerOptions {
                note_skin: String::new(),
                note_speed: NoteSpeed::Dynamic(1.0),
                perspective: Perspective::None,
                grade_layer: GradeLayer::Behind,
                grade_position: Percent(50.0),
            }),
            revision: 0,
            dirty_machine: false,
            dirty_players: false,
            base,
        }
    }

    fn enter_tree(&mut self) {
        let defaults = &config().defaults;
        self.machine = load_machine_settings(defaults);
        self.players = load_player_settings(defaults);
        audio::ensure_buses();
        self.apply_machine();
    }

    /// Persists at most once per frame, however many edits landed in it.
    fn process(&mut self, _delta: f64) {
        if self.dirty_machine {
            self.dirty_machine = false;
            save_user_data(MACHINE_SETTINGS_FILE, &self.machine);
        }
        if self.dirty_players {
            self.dirty_players = false;
            for player in PlayerId::iter() {
                save_user_data(player_settings_file(player), &self.players[player]);
            }
        }
    }
}

/// Settings that belong to the machine rather than to either player: the
/// key bindings for both player slots, the rig's timing calibration, and
/// its playback volumes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MachineSettings {
    pub keymap: Keymap,
    pub timing: TimingSettings,
    pub volume: VolumeSettings,
}

/// Playback volumes, each `0..=1`: `master` scales everything, the others
/// their own audio bus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VolumeSettings {
    pub master: f32,
    pub sfx: f32,
    pub music: f32,
}

/// Each player slot's own presentation options, indexed by player.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerSettings(PerPlayer<PlayerOptions>);

impl PlayerSettings {
    /// Both players on one set of options — for tools that run without the
    /// [`Settings`] singleton (nothing is persisted).
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
