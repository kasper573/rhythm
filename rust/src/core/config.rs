use crate::core::assets::asset_root;
use crate::core::input::{GameAction, Keymap};
use crate::core::settings::{NoteSpeed, PlayerOptions, TimingSettings, VolumeSettings};
use crate::core::units::{Beat, Percent, Seconds};
use godot::builtin::Color;
use serde::{Deserialize, Deserializer};
use std::sync::OnceLock;
use strum::IntoEnumIterator;

/// The loaded config, set once by the boot sequence via [`GameConfig::install`].
pub fn config() -> &'static GameConfig {
    CONFIG.get().expect("GameConfig installed at boot")
}

static CONFIG: OnceLock<GameConfig> = OnceLock::new();

#[derive(Debug, Clone, Deserialize)]
pub struct GameConfig {
    /// Case-insensitive search strings `(group, stepfile)` picking the
    /// wheel's default active row and expanded group. When nothing matches,
    /// the wheel defaults to the first stepfile of the first group.
    pub wheel_default: (String, String),
    pub defaults: SettingsDefaults,
    pub grading: GradingConfig,
    /// Tick track volume: `0..=1` attenuates, `1..=2` boosts. Capped at 2 so
    /// a config typo can never blow anyone's eardrums out.
    pub tick_volume: f32,
    /// The note denominations the game recognizes; notes on finer grids
    /// snap to the last entry. Note skins are cross-referenced by these.
    pub note_quants: Vec<u32>,
    pub speed_modifiers: SpeedModifiers,
    pub stage: StageConfig,
    pub lane_camera: LaneCameraConfig,
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

/// Default values for the user settings, in the settings' own types.
/// Fresh installs and settings files that predate a field resolve here —
/// user-configurable values have no defaults hardcoded in the code.
#[derive(Debug, Clone, Deserialize)]
pub struct SettingsDefaults {
    /// Must bind every action; the machine settings' keymap holds the
    /// players' overrides on top of it.
    pub keymap: Keymap,
    pub player_options: PlayerOptions,
    pub timing_options: TimingSettings,
    pub volume_options: VolumeSettings,
}

/// How play stages are fitted onto the screen.
#[derive(Debug, Clone, Deserialize)]
pub struct StageConfig {
    /// The largest arrow the game draws, in screen pixels; fields shrink
    /// below this only when the screen cannot fit their columns.
    pub max_arrow_size: f32,
    /// Width reserved on each screen edge that fields never enter: the
    /// health vials plus breathing room.
    pub margin_x: f32,
    /// Gap between adjacent fields, in column spacings.
    pub field_gap_columns: f32,
    /// Silence before the chart starts, giving the first notes room to
    /// scroll in.
    pub lead_in_seconds: Seconds,
    /// Padding between the screen edges and anchored stage furniture —
    /// the health vials keep this to their side edge and the note fields
    /// to the top edge, so everything hugging the frame lines up.
    pub screen_edge_padding: f32,
}

/// The perspective lane cameras (see the stepfile player's note field).
#[derive(Debug, Clone, Deserialize)]
pub struct LaneCameraConfig {
    /// Vertical field of view; the camera distance derives from it so the
    /// lane plane renders 1:1 with the 2D world.
    pub fov_degrees: f32,
    /// How far the Above/Below perspectives pitch the camera around the
    /// receptor row.
    pub tilt_degrees: f32,
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
    /// Hold let-go grace: life drains from full to dropped over this long
    /// once the panel is released.
    pub hold_grace_seconds: Seconds,
    /// Roll window: rolls drain constantly and each fresh step refills them.
    pub roll_grace_seconds: Seconds,
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
    #[serde(default)]
    pub glow: GradeGlow,
}

/// The grade text's shader shimmer (see the stepfile player's grade text):
/// an additive glow in `color`, oscillating at `strength` (0 = plain text).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct GradeGlow {
    #[serde(deserialize_with = "hex_color")]
    pub color: Color,
    pub strength: f32,
}

impl Default for GradeGlow {
    fn default() -> GradeGlow {
        GradeGlow {
            color: Color::WHITE,
            strength: 0.0,
        }
    }
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
    pub fn progress(&self, beat: Beat) -> f32 {
        self.ease(self.phase(beat))
    }

    /// Pulse intensity `0..=1` whose apex lands exactly on the cycle
    /// boundary (the configured beat): the phase is folded so intensity
    /// rises into the beat and falls away from it, shaped by the easing.
    pub fn pulse(&self, beat: Beat) -> f32 {
        self.ease((2.0 * self.phase(beat) - 1.0).abs())
    }

    /// Like [`pulse`](RhythmCycle::pulse), but striking: the rise into the
    /// apex takes only the last sliver of the cycle — practically instant
    /// — and everything before it eases out from the previous apex.
    pub fn strike(&self, beat: Beat) -> f32 {
        let phase = self.phase(beat);
        let decay = 1.0 - STRIKE_ATTACK;
        if phase >= decay {
            self.ease((phase - decay) / STRIKE_ATTACK)
        } else {
            self.ease(1.0 - phase / decay)
        }
    }

    /// Cycle phase `0..1`; .sm measures are four beats.
    fn phase(&self, beat: Beat) -> f32 {
        (beat.0 * self.speed / 4.0).rem_euclid(1.0) as f32
    }

    fn ease(&self, t: f32) -> f32 {
        let [x1, y1, x2, y2] = self.easing;
        cubic_bezier_ease(x1, y1, x2, y2, t)
    }
}

/// A CSS-style `cubic-bezier(x1, y1, x2, y2)` easing curve: the bezier
/// through (0,0) and (1,1) evaluated as y at horizontal position `t`,
/// solved by Newton iteration with a bisection fallback.
pub fn cubic_bezier_ease(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let axis = |a: f32, b: f32, s: f32| {
        let inverse = 1.0 - s;
        3.0 * inverse * inverse * s * a + 3.0 * inverse * s * s * b + s * s * s
    };
    let mut s = t;
    for _ in 0..8 {
        let error = axis(x1, x2, s) - t;
        if error.abs() < 1e-5 {
            return axis(y1, y2, s);
        }
        let inverse = 1.0 - s;
        let derivative =
            3.0 * inverse * inverse * x1 + 6.0 * inverse * s * (x2 - x1) + 3.0 * s * s * (1.0 - x2);
        if derivative.abs() < 1e-6 {
            break;
        }
        s = (s - error / derivative).clamp(0.0, 1.0);
    }
    let (mut low, mut high) = (0.0f32, 1.0f32);
    for _ in 0..24 {
        s = (low + high) / 2.0;
        if axis(x1, x2, s) < t {
            low = s;
        } else {
            high = s;
        }
    }
    axis(y1, y2, s)
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
                RawGradientPart::Stop(percent, hex) => parse_hex_color(&hex)
                    .map(|color| HealthColorStop {
                        percent: Percent(percent),
                        color,
                    })
                    .ok_or_else(|| format!("bad hex color {hex:?}")),
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
    /// The window's config key stays in milliseconds — the unit hand-tuned
    /// values read best in.
    #[serde(rename = "window_ms", deserialize_with = "seconds_from_millis")]
    pub window: Seconds,
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
    #[serde(default)]
    pub glow: GradeGlow,
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
    /// Loads, validates, and installs the config for [`config`] access.
    /// Panics on missing or invalid configuration: environments must be
    /// explicitly and correctly configured.
    pub fn install() {
        let path = asset_root().join("game_config.json");
        let bytes = crate::core::platform::platform()
            .read_asset(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let config: GameConfig = crate::core::jsonc::parse(&String::from_utf8_lossy(&bytes))
            .unwrap_or_else(|error| panic!("invalid {}: {error}", path.display()));
        config.validate(&path.display().to_string());
        CONFIG
            .set(config)
            .unwrap_or_else(|_| panic!("GameConfig is already installed"));
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
        self.grading
            .dynamic
            .last()
            .expect("grades are validated non-empty")
            .window
    }

    /// The grade earned by an input this far from the row, or `None` if the
    /// input misses every window (a harmless no-op input).
    fn grade_for_error(&self, error: Seconds) -> Option<GradeIndex> {
        let magnitude = error.abs();
        self.grading
            .dynamic
            .iter()
            .position(|grade| magnitude.0 <= grade.window.0)
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
                pair[0].window < pair[1].window,
                "{source}: grade windows must be sorted from smallest to largest"
            );
        }
        assert!(
            self.grading.dynamic[0].window > Seconds::ZERO,
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
            !self.speed_modifiers.constant.options.is_empty()
                && !self.speed_modifiers.dynamic.options.is_empty(),
            "{source}: speed_modifiers must offer at least one option each"
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
                crate::core::platform::platform().asset_exists(&asset_root().join(&rating.image)),
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
                self.defaults.keymap.binding(action).is_some(),
                "{source}: defaults.keymap must bind {action:?}"
            );
        }
        assert!(
            self.stage.max_arrow_size > 0.0
                && self.stage.margin_x >= 0.0
                && self.stage.field_gap_columns >= 0.0
                && self.stage.lead_in_seconds >= Seconds::ZERO
                && self.stage.screen_edge_padding >= 0.0,
            "{source}: stage values must not be negative (and max_arrow_size positive)"
        );
        assert!(
            (0.0..180.0).contains(&self.lane_camera.fov_degrees)
                && self.lane_camera.fov_degrees > 0.0
                && (0.0..90.0).contains(&self.lane_camera.tilt_degrees),
            "{source}: lane_camera needs fov in (0, 180) and tilt in [0, 90)"
        );
        assert!(
            self.grading.hold_grace_seconds > Seconds::ZERO
                && self.grading.roll_grace_seconds > Seconds::ZERO,
            "{source}: hold grace windows must be positive"
        );
        let volumes = &self.defaults.volume_options;
        assert!(
            [volumes.master, volumes.sfx, volumes.music]
                .iter()
                .all(|volume| (0.0..=1.0).contains(volume)),
            "{source}: defaults.volume_options must be within 0..=1"
        );
    }
}

fn seconds_from_millis<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Seconds, D::Error> {
    f64::deserialize(deserializer).map(Seconds::from_millis)
}

/// Parses `#RRGGBB`/`#RRGGBBAA` (hash optional) into a color.
fn parse_hex_color(text: &str) -> Option<Color> {
    let hex = text.trim_start_matches('#');
    if hex.len() != 6 && hex.len() != 8 {
        return None;
    }
    let byte = |index: usize| u8::from_str_radix(&hex[index..index + 2], 16).ok();
    let (red, green, blue) = (byte(0)?, byte(2)?, byte(4)?);
    let alpha = if hex.len() == 8 { byte(6)? } else { 255 };
    Some(Color::from_rgba8(red, green, blue, alpha))
}

fn hex_color<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Color, D::Error> {
    let text = String::deserialize(deserializer)?;
    parse_hex_color(&text)
        .ok_or_else(|| serde::de::Error::custom(format!("bad hex color {text:?}")))
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
