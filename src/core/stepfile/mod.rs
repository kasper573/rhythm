mod parse;
mod timing;

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
    /// Tags this parser has no dedicated field for, keyed by upper-case name.
    pub extra_tags: BTreeMap<String, String>,
}

impl Stepfile {
    pub fn parse(text: &str) -> Result<Stepfile, StepfileError> {
        parse::parse_stepfile(text)
    }

    /// Reads and parses the file, tolerating non-UTF-8 bytes (old simfiles
    /// often use legacy encodings for titles).
    pub fn load(path: &Path) -> Result<Stepfile, StepfileError> {
        let bytes = std::fs::read(path).map_err(|source| StepfileError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Stepfile::parse(&String::from_utf8_lossy(&bytes))
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
    #[error("stepfile has no parsable #NOTES charts")]
    NoCharts,
}

#[derive(Debug, Clone)]
pub struct Chart {
    pub steps_type: StepsType,
    pub description: String,
    pub difficulty: Difficulty,
    pub meter: u32,
    pub radar: Vec<f32>,
    pub columns: usize,
    /// Sorted by beat, then column.
    pub notes: Vec<Note>,
}

impl Chart {
    pub fn last_note_beat(&self) -> Option<Beat> {
        self.notes
            .iter()
            .map(|note| note.end_beat())
            .reduce(|a, b| if a.0 >= b.0 { a } else { b })
    }

    /// Walks the beat-sorted notes in runs of equal beat, so simultaneous
    /// arrows count as one jump.
    pub fn stats(&self) -> ChartStats {
        let mut stats = ChartStats::default();
        let mut run_start = 0;
        while run_start < self.notes.len() {
            let beat = self.notes[run_start].beat;
            let mut run_end = run_start;
            let mut steppable = 0;
            while run_end < self.notes.len() && self.notes[run_end].beat == beat {
                let note = &self.notes[run_end];
                match note.kind {
                    NoteKind::Hold { .. } | NoteKind::Roll { .. } => stats.holds += 1,
                    NoteKind::Mine => stats.mines += 1,
                    _ => {}
                }
                if note.is_steppable() {
                    steppable += 1;
                }
                run_end += 1;
            }
            stats.steps += steppable;
            if steppable >= 2 {
                stats.jumps += 1;
            }
            run_start = run_end;
        }
        stats
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChartStats {
    /// Every arrow the player must step on (hold heads included).
    pub steps: usize,
    /// Instants where two or more arrows must be stepped together.
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Note {
    pub beat: Beat,
    pub column: usize,
    /// The note value in music theory as notes-per-measure: 4 for quarter
    /// notes (on the beat), 8 for eighths, 12 for triplets, and so on. Never
    /// below 4: notes on coarser grids still land on the beat.
    pub quant: u32,
    pub kind: NoteKind,
}

impl Note {
    /// The beat where this note is over: the tail beat for holds and rolls,
    /// the note's own beat otherwise.
    pub fn end_beat(&self) -> Beat {
        match self.kind {
            NoteKind::Hold { end } | NoteKind::Roll { end } => end,
            _ => self.beat,
        }
    }

    pub fn is_steppable(&self) -> bool {
        matches!(
            self.kind,
            NoteKind::Tap | NoteKind::Hold { .. } | NoteKind::Roll { .. }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NoteKind {
    Tap,
    Hold { end: Beat },
    Roll { end: Beat },
    Mine,
    Lift,
    Fake,
}

/// One `#BGCHANGES` entry: switch the background to `file` at `beat`.
#[derive(Debug, Clone, PartialEq)]
pub struct BgChange {
    pub beat: Beat,
    pub file: String,
    pub rate: f64,
    /// Remaining `=`-separated parameters, kept verbatim.
    pub params: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DisplayBpm {
    Single(f64),
    Range(f64, f64),
    Random,
}
