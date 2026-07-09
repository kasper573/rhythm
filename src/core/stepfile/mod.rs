mod clock;
mod music_player;
mod parse;
mod timing;

pub use clock::StepfileClock;
pub use music_player::{Bgm, MusicPlayer, MusicPlayerPlugin};
pub use timing::StepfileTiming;

use crate::core::units::{Beat, Seconds};
use std::collections::BTreeMap;
use std::path::Path;
use strum::EnumString;

#[derive(Debug, Clone)]
pub struct Stepfile {
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub title_translit: String,
    pub subtitle_translit: String,
    pub artist_translit: String,
    pub credit: String,
    /// File names relative to the stepfile's own folder.
    pub banner: Option<String>,
    pub background: Option<String>,
    pub cd_title: Option<String>,
    pub music: Option<String>,
    pub sample_start: Seconds,
    pub sample_length: Seconds,
    pub selectable: bool,
    pub display_bpm: Option<DisplayBpm>,
    pub timing: StepfileTiming,
    pub bg_changes: Vec<BgChange>,
    pub charts: Vec<Chart>,
    /// Keyed by upper-case tag name.
    pub extra_tags: BTreeMap<String, String>,
}

impl Stepfile {
    pub fn parse(text: &str) -> Result<Stepfile, StepfileError> {
        parse::parse_stepfile(text)
    }

    /// Reads and parses the file, tolerating non-UTF-8 bytes (old simfiles
    /// often use legacy encodings for titles).
    pub fn load(path: &Path) -> Result<Stepfile, StepfileError> {
        let bytes = crate::core::platform::platform()
            .read_asset(path)
            .map_err(|source| StepfileError::Io {
                path: path.display().to_string(),
                source,
            })?;
        Stepfile::parse(&String::from_utf8_lossy(&bytes))
    }

    /// Where playback audibly is when looping the preview sample window,
    /// given the mixer's monotonic position: the raw position keeps
    /// growing across loops while the audio wraps every `sample_length`.
    pub fn sample_position(&self, raw: Seconds) -> Seconds {
        if self.sample_length.0 > 0.0 {
            self.sample_start + Seconds(raw.0.rem_euclid(self.sample_length.0))
        } else {
            self.sample_start + raw
        }
    }

    /// The chart played in single-player mode: the median-meter dance-single
    /// chart, falling back to the first chart of any type.
    pub fn preferred_chart(&self) -> Option<&Chart> {
        let mut singles: Vec<&Chart> = self
            .charts
            .iter()
            .filter(|c| c.steps_type == StepsType::DanceSingle)
            .collect();
        if singles.is_empty() {
            return self.charts.first();
        }
        singles.sort_by_key(|c| c.meter);
        Some(singles[singles.len() / 2])
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StepfileError {
    #[error("failed to read {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("stepfile has no valid #BPMS")]
    NoBpms,
}

#[derive(Debug, Clone)]
pub struct Chart {
    pub steps_type: StepsType,
    pub description: String,
    pub difficulty: Difficulty,
    pub meter: u32,
    pub radar: Vec<f32>,
    pub columns: usize,
    /// Sorted by beat. The row is the unit the game grades: every arrow in
    /// it must be stepped for the row to count, and rows with two or more
    /// arrows are the jumps shown on the file select.
    pub rows: Vec<Row>,
    /// Sorted by beat.
    pub mines: Vec<Mine>,
}

impl Chart {
    pub fn last_note_beat(&self) -> Option<Beat> {
        self.rows
            .iter()
            .flat_map(|row| row.arrows.iter().map(|arrow| arrow.end_beat(row.beat)))
            .chain(self.mines.iter().map(|mine| mine.beat))
            .reduce(|a, b| if a.0 >= b.0 { a } else { b })
    }

    pub fn stats(&self) -> ChartStats {
        ChartStats {
            steps: self.rows.iter().map(|row| row.arrows.len()).sum(),
            jumps: self.rows.iter().filter(|row| row.is_jump()).count(),
            holds: self
                .rows
                .iter()
                .flat_map(|row| &row.arrows)
                .filter(|arrow| arrow.tail.is_some())
                .count(),
            mines: self.mines.len(),
        }
    }
}

/// Simultaneous arrows on one beat, stepped and graded as a single unit.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub beat: Beat,
    /// The row's note value in music theory as notes-per-measure: 4 for
    /// quarter notes (on the beat), 8 for eighths, 12 for triplets, and so
    /// on. Never below 4: rows on coarser grids still land on the beat.
    pub quant: u32,
    /// Sorted by column; never empty.
    pub arrows: Vec<Arrow>,
}

impl Row {
    pub fn is_jump(&self) -> bool {
        self.arrows.len() >= 2
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Arrow {
    pub column: usize,
    /// Holds and rolls sustain to a tail; plain taps have none.
    pub tail: Option<Tail>,
}

impl Arrow {
    /// The beat where this arrow is over: the tail beat for holds and
    /// rolls, the row's own beat otherwise.
    pub fn end_beat(&self, row_beat: Beat) -> Beat {
        self.tail.map(|tail| tail.end).unwrap_or(row_beat)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tail {
    pub end: Beat,
    pub roll: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mine {
    pub beat: Beat,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChartStats {
    /// Every arrow the player must step on (hold heads included).
    pub steps: usize,
    /// Rows where two or more arrows must be stepped together.
    pub jumps: usize,
    /// A hold's start and end pair counts as one hold (rolls included).
    pub holds: usize,
    pub mines: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, EnumString)]
#[strum(ascii_case_insensitive)]
pub enum StepsType {
    #[strum(serialize = "dance-single")]
    DanceSingle,
    #[strum(serialize = "dance-double")]
    DanceDouble,
    #[strum(serialize = "dance-solo")]
    DanceSolo,
    #[strum(serialize = "dance-couple")]
    DanceCouple,
    #[strum(default)]
    Other(String),
}

impl StepsType {
    /// Column count for the known styles; unknown styles infer their column
    /// count from the note data instead.
    pub fn columns(&self) -> Option<usize> {
        match self {
            StepsType::DanceSingle => Some(4),
            StepsType::DanceDouble | StepsType::DanceCouple => Some(8),
            StepsType::DanceSolo => Some(6),
            StepsType::Other(_) => None,
        }
    }
}

/// The canonical names plus the legacy aliases still found in old files.
#[derive(Debug, Clone, PartialEq, Eq, EnumString)]
#[strum(ascii_case_insensitive)]
pub enum Difficulty {
    Beginner,
    #[strum(serialize = "easy", serialize = "basic", serialize = "light")]
    Easy,
    #[strum(
        serialize = "medium",
        serialize = "another",
        serialize = "trick",
        serialize = "standard"
    )]
    Medium,
    #[strum(
        serialize = "hard",
        serialize = "ssr",
        serialize = "maniac",
        serialize = "heavy"
    )]
    Hard,
    #[strum(
        serialize = "challenge",
        serialize = "smaniac",
        serialize = "expert",
        serialize = "oni"
    )]
    Challenge,
    Edit,
    #[strum(default)]
    Other(String),
}

impl Difficulty {
    /// Canonical easiest-to-hardest ordering, used to keep the selected
    /// difficulty stable while browsing between stepfiles.
    pub fn rank(&self) -> u8 {
        match self {
            Difficulty::Beginner => 0,
            Difficulty::Easy => 1,
            Difficulty::Medium => 2,
            Difficulty::Hard => 3,
            Difficulty::Challenge => 4,
            Difficulty::Edit => 5,
            Difficulty::Other(_) => 6,
        }
    }
}

/// One `#BGCHANGES` entry: switch the background to `file` at `beat`.
#[derive(Debug, Clone, PartialEq)]
pub struct BgChange {
    pub beat: Beat,
    pub file: String,
    /// Fade into this background instead of cutting to it.
    pub crossfade: bool,
    /// Whether a movie loops; otherwise it plays once and holds its last
    /// frame.
    pub loops: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DisplayBpm {
    Single(f64),
    Range(f64, f64),
    Random,
}
