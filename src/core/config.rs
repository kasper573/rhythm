use crate::core::assets::asset_root;
use crate::core::input::GameAction;
use crate::core::note_field::NoteSpeed;
use crate::core::units::{Percent, Seconds};
use bevy::math::cubic_splines::CubicSegment;
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
    pub grading: GradingConfig,
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
    /// Combo at which the arrow flash switches to its brighter, snappier
    /// variant.
    pub bright_arrow_flash_combo: u32,
    /// Every play session starts at full health; grades then apply their
    /// `health_offset`, and draining to zero fails the session.
    pub player_max_health: u32,
    pub healthbar: HealthBarConfig,
    /// Rating rules tried in order: the first match wins, so earlier
    /// entries take priority.
    pub ratings: Vec<RatingDef>,
}

/// One `{ image, point_percentage | all_grades_gte }` config entry; the
/// two rule fields are mutually exclusive.
#[derive(Debug, Clone, Deserialize)]
#[serde(try_from = "RawRatingDef")]
pub struct RatingDef {
    /// Image path under the asset root.
    pub image: String,
    pub kind: RatingKind,
}

#[derive(Debug, Clone)]
pub enum RatingKind {
    /// Matches a score of at least this percentage.
    PointPercentage(u8),
    /// Matches when every row of the chart was graded at least this
    /// (dynamic) grade — partial runs never match.
    AllGradesGte(String),
}

#[derive(Deserialize)]
struct RawRatingDef {
    image: String,
    point_percentage: Option<u8>,
    all_grades_gte: Option<String>,
}

impl TryFrom<RawRatingDef> for RatingDef {
    type Error = String;

    fn try_from(raw: RawRatingDef) -> Result<RatingDef, String> {
        let kind = match (raw.point_percentage, raw.all_grades_gte) {
            (Some(percent), None) => RatingKind::PointPercentage(percent),
            (None, Some(grade)) => RatingKind::AllGradesGte(grade),
            _ => {
                return Err(format!(
                    "rating {}: exactly one of point_percentage/all_grades_gte required",
                    raw.image
                ));
            }
        };
        Ok(RatingDef {
            image: raw.image,
            kind,
        })
    }
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
pub struct GradingConfig {
    /// The timed grades, ordered best to worst (smallest window first).
    pub dynamic: Vec<DynamicGradeDef>,
    pub fixed: FixedGrades,
}

/// The built-in grades: rows that expired unstepped (miss), and holds
/// kept to the end (ok) or dropped (ng).
#[derive(Debug, Clone, Deserialize)]
pub struct FixedGrades {
    pub miss: FixedGradeDef,
    pub ok: FixedGradeDef,
    pub ng: FixedGradeDef,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FixedGradeDef {
    pub name: String,
    #[serde(deserialize_with = "hex_color")]
    pub color: Color,
    pub health_offset: i32,
    pub points: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HealthBarConfig {
    /// The vial's glass glow, pulsing with the music.
    pub glow: RhythmCycle,
    /// The liquid gradient's looping scroll, ebbing with the music.
    pub liquid: RhythmCycle,
    /// Gradient presets keyed by the health percentage they take effect
    /// at; the active preset is the last one at or below current health.
    /// Sorted ascending.
    pub colors: Vec<HealthGradient>,
}

/// Fraction of a [`RhythmCycle::strike`] cycle spent rising into the apex.
const STRIKE_ATTACK: f32 = 0.06;

/// An animation cycle locked to the music. `speed` counts cycles per
/// measure — 4 cycles once every beat, 1 once every measure — and
/// `easing` is a CSS-style cubic bezier `[x1, y1, x2, y2]` shaping the
/// progression within each cycle.
#[derive(Debug, Clone, Deserialize)]
pub struct RhythmCycle {
    pub speed: f64,
    pub easing: [f32; 4],
}

impl RhythmCycle {
    /// Eased progression through the current cycle, `0..=1`, continuous
    /// across cycle boundaries for animations that wrap.
    pub fn progress(&self, beat: f64) -> f32 {
        self.ease(self.phase(beat))
    }

    /// Pulse intensity `0..=1` whose apex lands exactly on the cycle
    /// boundary (the configured beat): the phase is folded so intensity
    /// rises into the beat and falls away from it, shaped by the easing.
    pub fn pulse(&self, beat: f64) -> f32 {
        self.ease((2.0 * self.phase(beat) - 1.0).abs())
    }

    /// Like [`pulse`](RhythmCycle::pulse), but striking: the rise into the
    /// apex takes only the last sliver of the cycle — practically instant
    /// — and everything before it eases out from the previous apex.
    pub fn strike(&self, beat: f64) -> f32 {
        let phase = self.phase(beat);
        let decay = 1.0 - STRIKE_ATTACK;
        if phase >= decay {
            self.ease((phase - decay) / STRIKE_ATTACK)
        } else {
            self.ease(1.0 - phase / decay)
        }
    }

    /// Cycle phase `0..1`; .sm measures are four beats.
    fn phase(&self, beat: f64) -> f32 {
        (beat * self.speed / 4.0).rem_euclid(1.0) as f32
    }

    fn ease(&self, t: f32) -> f32 {
        let [x1, y1, x2, y2] = self.easing;
        CubicSegment::new_bezier_easing((x1, y1), (x2, y2)).ease(t)
    }
}

impl HealthBarConfig {
    pub fn gradient_at(&self, health: Percent) -> &HealthGradient {
        self.colors
            .iter()
            .rev()
            .find(|gradient| gradient.min_health <= health)
            .unwrap_or(&self.colors[0])
    }
}

/// One `[health, [percent, "#RRGGBB"], ...]` healthbar config entry: the
/// bottom-to-top color stops used from `min_health` percent upward.
#[derive(Debug, Clone, Deserialize)]
#[serde(try_from = "RawHealthGradient")]
pub struct HealthGradient {
    pub min_health: Percent,
    /// Sorted ascending; the spectrum outside the outermost stops extends
    /// flat, like CSS gradients.
    pub stops: Vec<HealthColorStop>,
}

#[derive(Debug, Clone, Copy)]
pub struct HealthColorStop {
    /// Position along the gradient axis, bottom to top.
    pub percent: Percent,
    pub color: Color,
}

#[derive(Deserialize)]
struct RawHealthGradient(Vec<RawGradientPart>);

#[derive(Deserialize)]
#[serde(untagged)]
enum RawGradientPart {
    MinHealth(f32),
    Stop(f32, String),
}

impl TryFrom<RawHealthGradient> for HealthGradient {
    type Error = String;

    fn try_from(raw: RawHealthGradient) -> Result<HealthGradient, String> {
        let mut parts = raw.0.into_iter();
        let Some(RawGradientPart::MinHealth(min_health)) = parts.next() else {
            return Err("healthbar entry must start with its health threshold".into());
        };
        let stops = parts
            .map(|part| match part {
                RawGradientPart::Stop(percent, hex) => Srgba::hex(&hex)
                    .map(|color| HealthColorStop {
                        percent: Percent(percent),
                        color: Color::Srgba(color),
                    })
                    .map_err(|error| format!("bad hex color {hex:?}: {error}")),
                RawGradientPart::MinHealth(value) => {
                    Err(format!("expected a [percent, color] stop, got {value}"))
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(HealthGradient {
            min_health: Percent(min_health),
            stops,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DynamicGradeDef {
    pub name: String,
    pub window_ms: f64,
    #[serde(deserialize_with = "hex_color")]
    pub color: Color,
    #[serde(default)]
    pub breaks_combo: bool,
    #[serde(default, deserialize_with = "timing_feedback")]
    pub timing_feedback: TimingFeedback,
    /// Arrow flash color at the receptors when a step resolves at this
    /// grade. Grades without one flash nothing, and their arrows stay on
    /// screen and scroll past instead of vanishing.
    #[serde(default, deserialize_with = "optional_hex_color")]
    pub arrow_flash: Option<Color>,
    /// Applied to the player's health when this grade is given.
    pub health_offset: i32,
    /// Score points earned when this grade is given.
    pub points: u32,
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

/// Index into [`GradingConfig::dynamic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GradeIndex(pub usize);

/// An outcome classified under the config: one of the timed grades, or the
/// built-in Miss.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grade {
    Hit(GradeIndex),
    Miss,
}

/// What actually happened to one row: the input's signed timing error, or
/// expiry without any input. The raw error is the single source of truth;
/// the grade it represents is derived on demand via [`GameConfig::grade`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RowOutcome {
    /// The row was hit `error` away from its moment; positive = early.
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

    pub fn health_offset(&self, grade: Grade) -> i32 {
        match grade {
            Grade::Hit(grade) => self.grading.dynamic[grade.0].health_offset,
            Grade::Miss => self.grading.fixed.miss.health_offset,
        }
    }

    pub fn points(&self, grade: Grade) -> u32 {
        match grade {
            Grade::Hit(grade) => self.grading.dynamic[grade.0].points,
            Grade::Miss => self.grading.fixed.miss.points,
        }
    }

    /// The score as a percentage of a perfect run: every row earning the
    /// highest configured points and every hold kept.
    pub fn score_percent(&self, points: u32, rows: u32, holds: u32) -> Percent {
        let best = self
            .grading
            .dynamic
            .iter()
            .map(|grade| grade.points)
            .max()
            .unwrap_or(0);
        let max = rows as f64 * best as f64 + holds as f64 * self.grading.fixed.ok.points as f64;
        if max <= 0.0 {
            return Percent(0.0);
        }
        Percent((points as f64 / max * 100.0) as f32)
    }

    /// The first rating rule the result matches, in config order.
    /// `worst_grade` is the worst grade any row earned, or `None` when
    /// unknown or the run graded only part of the chart.
    pub fn rating(&self, percent: Percent, worst_grade: Option<Grade>) -> &RatingDef {
        self.ratings
            .iter()
            .find(|rating| match &rating.kind {
                RatingKind::PointPercentage(threshold) => percent.0 >= *threshold as f32,
                RatingKind::AllGradesGte(name) => match worst_grade {
                    Some(Grade::Hit(worst)) => {
                        let threshold = self
                            .grading
                            .dynamic
                            .iter()
                            .position(|grade| grade.name == *name)
                            .expect("validated: all_grades_gte names a dynamic grade");
                        worst.0 <= threshold
                    }
                    Some(Grade::Miss) | None => false,
                },
            })
            .expect("validated: a point_percentage 0 rating always matches")
    }

    /// The widest grading window, which doubles as the miss/expiry window:
    /// an unpressed note expires once it is this far in the past.
    pub fn widest_window(&self) -> Seconds {
        Seconds::from_millis(
            self.grading
                .dynamic
                .last()
                .expect("grades are validated non-empty")
                .window_ms,
        )
    }

    /// The grade earned by an input this far from the row, or `None` if the
    /// input misses every window (a harmless no-op input).
    fn grade_for_error(&self, error: Seconds) -> Option<GradeIndex> {
        let magnitude = error.abs();
        self.grading
            .dynamic
            .iter()
            .position(|grade| magnitude.0 <= Seconds::from_millis(grade.window_ms).0)
            .map(GradeIndex)
    }

    pub fn breaks_combo(&self, grade: Grade) -> bool {
        match grade {
            Grade::Hit(grade) => self.grading.dynamic[grade.0].breaks_combo,
            Grade::Miss => true,
        }
    }

    pub fn grade(&self, outcome: RowOutcome) -> Grade {
        match outcome {
            RowOutcome::Hit { error } => Grade::Hit(
                self.grade_for_error(error)
                    .expect("hits are only recorded inside the widest grading window"),
            ),
            RowOutcome::Miss => Grade::Miss,
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
            !self.grading.dynamic.is_empty(),
            "{source}: grading.dynamic must not be empty"
        );
        for pair in self.grading.dynamic.windows(2) {
            assert!(
                pair[0].window_ms < pair[1].window_ms,
                "{source}: grade windows must be sorted from smallest to largest"
            );
        }
        assert!(
            self.grading.dynamic[0].window_ms > 0.0,
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
            self.player_max_health > 0,
            "{source}: player_max_health must be positive"
        );
        assert!(
            !self.healthbar.colors.is_empty(),
            "{source}: healthbar colors must not be empty"
        );
        assert!(
            self.ratings
                .iter()
                .any(|rating| matches!(rating.kind, RatingKind::PointPercentage(0))),
            "{source}: ratings need a point_percentage 0 entry so every score earns one"
        );
        for rating in &self.ratings {
            assert!(
                asset_root().join(&rating.image).exists(),
                "{source}: rating image {} does not exist",
                rating.image
            );
            if let RatingKind::AllGradesGte(name) = &rating.kind {
                assert!(
                    self.grading.dynamic.iter().any(|grade| grade.name == *name),
                    "{source}: all_grades_gte {name:?} is not a dynamic grade"
                );
            }
        }
        for cycle in [&self.healthbar.glow, &self.healthbar.liquid] {
            assert!(
                cycle.speed > 0.0,
                "{source}: healthbar cycle speeds must be positive"
            );
            assert!(
                (0.0..=1.0).contains(&cycle.easing[0]) && (0.0..=1.0).contains(&cycle.easing[2]),
                "{source}: healthbar easing x control points must be within 0..=1"
            );
        }
        for pair in self.healthbar.colors.windows(2) {
            assert!(
                pair[0].min_health < pair[1].min_health,
                "{source}: healthbar entries must be sorted by health threshold"
            );
        }
        for gradient in &self.healthbar.colors {
            assert!(
                !gradient.stops.is_empty(),
                "{source}: healthbar entries need at least one color stop"
            );
            for pair in gradient.stops.windows(2) {
                assert!(
                    pair[0].percent < pair[1].percent,
                    "{source}: healthbar color stops must be sorted ascending"
                );
            }
        }
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

fn optional_hex_color<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Color>, D::Error> {
    hex_color(deserializer).map(Some)
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
