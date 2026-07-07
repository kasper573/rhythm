use crate::core::assets::asset_root;
use crate::core::input::GameAction;
use crate::core::note_field::NoteSpeed;
use crate::core::units::Seconds;
use bevy::prelude::*;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
use strum::IntoEnumIterator;

#[derive(Resource, Debug, Clone, Deserialize)]
pub struct GameConfig {
    /// Case-insensitive search strings `(group, stepfile)` picking the
    /// wheel's default active row and expanded group. When nothing matches,
    /// the wheel defaults to the first stepfile of the first group.
    pub wheel_default: (String, String),
    /// Grading windows ordered best to worst (smallest window first).
    pub grades: Vec<GradeDef>,
    pub miss_appearance: MissAppearance,
    /// Tick track volume: `0..=1` attenuates, `1..=2` boosts. Capped at 2 so
    /// a config typo can never blow anyone's eardrums out.
    pub tick_volume: f32,
    /// The note denominations the game recognizes; notes on finer grids
    /// snap to the last entry. Note skins are cross-referenced by these.
    pub note_quants: Vec<u32>,
    /// Must bind every action; the settings hold the player's overrides.
    pub default_keymap: BTreeMap<GameAction, KeyCode>,
    pub speed_modifiers: SpeedModifiers,
    pub default_stepfile_options: StepfileOptions,
}

/// The player's presentation choices for playing stepfiles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepfileOptions {
    /// Folder name of the note skin under `assets/note_skins`.
    pub note_skin: String,
    pub note_speed: NoteSpeed,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SpeedModifiers {
    pub constant: SpeedModifierSet,
    pub dynamic: SpeedModifierSet,
}

impl SpeedModifiers {
    pub fn set(&self, speed: NoteSpeed) -> &SpeedModifierSet {
        match speed {
            NoteSpeed::Constant(_) => &self.constant,
            NoteSpeed::Dynamic(_) => &self.dynamic,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpeedModifierSet {
    pub options: Vec<f32>,
    pub default: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MissAppearance {
    pub name: String,
    #[serde(deserialize_with = "hex_color")]
    pub color: Color,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GradeDef {
    pub name: String,
    pub window_ms: f64,
    #[serde(deserialize_with = "hex_color")]
    pub color: Color,
    #[serde(default)]
    pub breaks_combo: bool,
    #[serde(default, deserialize_with = "timing_feedback")]
    pub timing_feedback: TimingFeedback,
}

/// In the config: `false`, `true`, or `"ms"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimingFeedback {
    #[default]
    Off,
    /// A dash marks the side: leading when early, trailing when late.
    Sign,
    /// The signed offset in milliseconds, e.g. `(-32ms) Good`.
    Millis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GradeIndex(pub usize);

/// The outcome of one note: either a configured grade, or the special
/// always-existing Miss for notes that expired without input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Judgment {
    Grade(GradeIndex),
    Miss,
}

/// What actually happened to one note: the input's signed timing error, or
/// expiry without any input. The raw error is the single source of truth;
/// the grade it represents is derived on demand via [`GameConfig::judge`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StepOutcome {
    /// The note was hit `error` away from its moment; positive = early.
    Hit {
        error: Seconds,
    },
    Miss,
}

impl GameConfig {
    /// Panics on missing or invalid configuration: environments must be
    /// explicitly and correctly configured.
    pub fn load() -> GameConfig {
        let path = asset_root().join("game_config.json");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let config: GameConfig = crate::core::jsonc::parse(&text)
            .unwrap_or_else(|error| panic!("invalid {}: {error}", path.display()));
        config.validate(&path.display().to_string());
        config
    }

    pub fn window(&self, grade: GradeIndex) -> Seconds {
        Seconds::from_millis(self.grades[grade.0].window_ms)
    }

    /// The widest grading window, which doubles as the miss/expiry window:
    /// an unpressed note expires once it is this far in the past.
    pub fn widest_window(&self) -> Seconds {
        Seconds::from_millis(
            self.grades
                .last()
                .expect("grades are validated non-empty")
                .window_ms,
        )
    }

    /// The grade earned by an input this far from the note, or `None` if the
    /// input misses every window (a harmless no-op input).
    pub fn grade_for_error(&self, error: Seconds) -> Option<GradeIndex> {
        let magnitude = error.abs();
        self.grades
            .iter()
            .position(|grade| magnitude.0 <= Seconds::from_millis(grade.window_ms).0)
            .map(GradeIndex)
    }

    pub fn breaks_combo(&self, judgment: Judgment) -> bool {
        match judgment {
            Judgment::Grade(grade) => self.grades[grade.0].breaks_combo,
            Judgment::Miss => true,
        }
    }

    pub fn judge(&self, outcome: StepOutcome) -> Judgment {
        match outcome {
            StepOutcome::Hit { error } => Judgment::Grade(
                self.grade_for_error(error)
                    .expect("hits are only recorded inside the widest grading window"),
            ),
            StepOutcome::Miss => Judgment::Miss,
        }
    }

    /// Snaps a parsed note value to the recognized denominations, falling
    /// back to the finest recognized one.
    pub fn recognized_quant(&self, quant: u32) -> u32 {
        if self.note_quants.contains(&quant) {
            quant
        } else {
            *self
                .note_quants
                .last()
                .expect("note_quants are validated non-empty")
        }
    }

    fn validate(&self, source: &str) {
        assert!(
            !self.grades.is_empty(),
            "{source}: grades must not be empty"
        );
        for pair in self.grades.windows(2) {
            assert!(
                pair[0].window_ms < pair[1].window_ms,
                "{source}: grade windows must be sorted from smallest to largest"
            );
        }
        assert!(
            self.grades[0].window_ms > 0.0,
            "{source}: grade windows must be positive"
        );
        assert!(
            (0.0..=2.0).contains(&self.tick_volume),
            "{source}: tick_volume must be between 0 and 2"
        );
        assert!(
            !self.note_quants.is_empty(),
            "{source}: note_quants must not be empty"
        );
        assert!(
            self.note_quants.iter().all(|quant| *quant > 0),
            "{source}: note_quants must be positive"
        );
        for action in GameAction::iter() {
            assert!(
                self.default_keymap.contains_key(&action),
                "{source}: default_keymap must bind {action:?}"
            );
        }
    }
}

fn hex_color<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Color, D::Error> {
    let text = String::deserialize(deserializer)?;
    Srgba::hex(&text)
        .map(Color::Srgba)
        .map_err(|error| serde::de::Error::custom(format!("bad hex color {text:?}: {error}")))
}

fn timing_feedback<'de, D: Deserializer<'de>>(deserializer: D) -> Result<TimingFeedback, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Toggle(bool),
        Mode(String),
    }
    match Raw::deserialize(deserializer)? {
        Raw::Toggle(false) => Ok(TimingFeedback::Off),
        Raw::Toggle(true) => Ok(TimingFeedback::Sign),
        Raw::Mode(mode) if mode == "ms" => Ok(TimingFeedback::Millis),
        Raw::Mode(mode) => Err(serde::de::Error::custom(format!(
            "unknown timing_feedback {mode:?}: expected true, false, or \"ms\""
        ))),
    }
}
