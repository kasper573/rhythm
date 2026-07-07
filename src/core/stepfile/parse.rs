use super::{
    BgChange, Chart, DisplayBpm, Note, NoteKind, Stepfile, StepfileError, StepfileTiming, StepsType,
};
use crate::core::units::{Beat, Seconds};
use std::collections::BTreeMap;

pub(super) fn parse_stepfile(text: &str) -> Result<Stepfile, StepfileError> {
    let text = strip_comments(text);

    let mut title = String::new();
    let mut subtitle = String::new();
    let mut artist = String::new();
    let mut title_translit = String::new();
    let mut subtitle_translit = String::new();
    let mut artist_translit = String::new();
    let mut credit = String::new();
    let mut banner = None;
    let mut background = None;
    let mut cd_title = None;
    let mut music = None;
    let mut offset = Seconds::ZERO;
    let mut sample_start = Seconds::ZERO;
    let mut sample_length = Seconds(10.0);
    let mut selectable = true;
    let mut display_bpm = None;
    let mut bpms = Vec::new();
    let mut stops = Vec::new();
    let mut bg_changes = Vec::new();
    let mut charts = Vec::new();
    let mut extra_tags = BTreeMap::new();

    for (name, value) in scan_tags(&text) {
        let trimmed = value.trim();
        match name.as_str() {
            "TITLE" => title = trimmed.to_string(),
            "SUBTITLE" => subtitle = trimmed.to_string(),
            "ARTIST" => artist = trimmed.to_string(),
            "TITLETRANSLIT" => title_translit = trimmed.to_string(),
            "SUBTITLETRANSLIT" => subtitle_translit = trimmed.to_string(),
            "ARTISTTRANSLIT" => artist_translit = trimmed.to_string(),
            "CREDIT" => credit = trimmed.to_string(),
            "BANNER" => banner = non_empty(trimmed),
            "BACKGROUND" => background = non_empty(trimmed),
            "CDTITLE" => cd_title = non_empty(trimmed),
            "MUSIC" => music = non_empty(trimmed),
            "OFFSET" => offset = Seconds(parse_number(trimmed)),
            "SAMPLESTART" => sample_start = Seconds(parse_number(trimmed)),
            "SAMPLELENGTH" => sample_length = Seconds(parse_number(trimmed)),
            "SELECTABLE" => selectable = !trimmed.eq_ignore_ascii_case("no"),
            "DISPLAYBPM" => display_bpm = parse_display_bpm(trimmed),
            "BPMS" => bpms = parse_beat_number_pairs(trimmed),
            // Delays differ from stops only for notes exactly on the pause
            // beat, which classic .sm files don't rely on.
            "STOPS" | "DELAYS" | "FREEZES" => {
                stops.extend(
                    parse_beat_number_pairs(trimmed)
                        .into_iter()
                        .map(|(beat, duration)| (beat, Seconds(duration))),
                );
            }
            "BGCHANGES" | "ANIMATIONS" => bg_changes = parse_bg_changes(trimmed),
            "NOTES" => {
                if let Some(chart) = parse_chart(&value) {
                    charts.push(chart);
                }
            }
            _ => {
                extra_tags.insert(name, trimmed.to_string());
            }
        }
    }

    if !bpms.iter().any(|(_, bpm)| *bpm > 0.0) {
        return Err(StepfileError::NoBpms);
    }
    if charts.is_empty() {
        return Err(StepfileError::NoCharts);
    }

    Ok(Stepfile {
        title,
        subtitle,
        artist,
        title_translit,
        subtitle_translit,
        artist_translit,
        credit,
        banner,
        background,
        cd_title,
        music,
        sample_start,
        sample_length,
        selectable,
        display_bpm,
        timing: StepfileTiming::new(offset, &bpms, &stops),
        bg_changes,
        charts,
        extra_tags,
    })
}

/// Yields `(NAME, value)` for each `#NAME:value;` tag, with names upper-cased.
fn scan_tags(text: &str) -> Vec<(String, String)> {
    let mut tags = Vec::new();
    let mut rest = text;
    while let Some(hash) = rest.find('#') {
        rest = &rest[hash + 1..];
        let Some(colon) = rest.find(':') else { break };
        let name = rest[..colon].trim().to_uppercase();
        rest = &rest[colon + 1..];
        let end = rest.find(';').unwrap_or(rest.len());
        tags.push((name, rest[..end].to_string()));
        rest = rest.get(end + 1..).unwrap_or("");
    }
    tags
}

fn strip_comments(text: &str) -> String {
    text.lines()
        .map(|line| line.split_once("//").map_or(line, |(before, _)| before))
        .collect::<Vec<_>>()
        .join("\n")
}

fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

fn parse_number(value: &str) -> f64 {
    value.trim().parse().unwrap_or(0.0)
}

fn parse_beat_number_pairs(value: &str) -> Vec<(Beat, f64)> {
    value
        .split(',')
        .filter_map(|entry| {
            let (beat, number) = entry.split_once('=')?;
            Some((Beat(beat.trim().parse().ok()?), number.trim().parse().ok()?))
        })
        .collect()
}

fn parse_display_bpm(value: &str) -> Option<DisplayBpm> {
    if value.is_empty() {
        return None;
    }
    if value.starts_with('*') {
        return Some(DisplayBpm::Random);
    }
    match value.split_once(':') {
        Some((low, high)) => Some(DisplayBpm::Range(
            low.trim().parse().ok()?,
            high.trim().parse().ok()?,
        )),
        None => Some(DisplayBpm::Single(value.parse().ok()?)),
    }
}

fn parse_bg_changes(value: &str) -> Vec<BgChange> {
    value
        .split(',')
        .filter_map(|entry| {
            let mut fields = entry.trim().split('=');
            let beat = Beat(fields.next()?.trim().parse().ok()?);
            let file = fields.next()?.trim().to_string();
            if file.is_empty() {
                return None;
            }
            let rate = fields
                .next()
                .and_then(|rate| rate.trim().parse().ok())
                .unwrap_or(1.0);
            Some(BgChange {
                beat,
                file,
                rate,
                params: fields.map(|field| field.trim().to_string()).collect(),
            })
        })
        .collect()
}

fn parse_chart(value: &str) -> Option<Chart> {
    let parts: Vec<&str> = value.splitn(6, ':').collect();
    let [steps_type, description, difficulty, meter, radar, note_data] = parts.as_slice() else {
        return None;
    };

    let steps_type: StepsType = steps_type
        .trim()
        .parse()
        .expect("parsing with a default variant is infallible");
    let (columns, notes) = parse_note_data(note_data, steps_type.columns())?;

    Some(Chart {
        steps_type,
        description: description.trim().to_string(),
        difficulty: difficulty
            .trim()
            .parse()
            .expect("parsing with a default variant is infallible"),
        meter: meter.trim().parse().unwrap_or(0),
        radar: radar
            .split(',')
            .filter_map(|radar_value| radar_value.trim().parse().ok())
            .collect(),
        columns,
        notes,
    })
}

/// Parses measure-based note data. Returns the column count (inferred from
/// the first row when the steps type doesn't dictate one) and the notes.
fn parse_note_data(data: &str, known_columns: Option<usize>) -> Option<(usize, Vec<Note>)> {
    let measures: Vec<Vec<&str>> = data
        .split(',')
        .map(|measure| {
            measure
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && line.bytes().all(is_note_char))
                .collect()
        })
        .collect();

    let columns = known_columns.or_else(|| {
        measures
            .iter()
            .flatten()
            .next()
            .map(|first_row| first_row.len())
    })?;

    let mut notes = Vec::new();
    let mut open_holds: Vec<Option<(Beat, u32, bool)>> = vec![None; columns];

    for (measure_index, rows) in measures.iter().enumerate() {
        let row_count = rows.len();
        for (row_index, row) in rows.iter().enumerate() {
            let beat = Beat(measure_index as f64 * 4.0 + row_index as f64 * 4.0 / row_count as f64);
            let quant = quantization(row_index, row_count);
            for (column, char) in row.bytes().enumerate().take(columns) {
                match char {
                    b'1' => notes.push(Note {
                        beat,
                        column,
                        quant,
                        kind: NoteKind::Tap,
                    }),
                    b'2' => open_holds[column] = Some((beat, quant, false)),
                    b'4' => open_holds[column] = Some((beat, quant, true)),
                    b'3' => {
                        if let Some((head, head_quant, is_roll)) = open_holds[column].take() {
                            notes.push(Note {
                                beat: head,
                                column,
                                quant: head_quant,
                                kind: if is_roll {
                                    NoteKind::Roll { end: beat }
                                } else {
                                    NoteKind::Hold { end: beat }
                                },
                            });
                        }
                    }
                    b'M' | b'm' => notes.push(Note {
                        beat,
                        column,
                        quant,
                        kind: NoteKind::Mine,
                    }),
                    b'L' | b'l' => notes.push(Note {
                        beat,
                        column,
                        quant,
                        kind: NoteKind::Lift,
                    }),
                    b'F' | b'f' => notes.push(Note {
                        beat,
                        column,
                        quant,
                        kind: NoteKind::Fake,
                    }),
                    _ => {}
                }
            }
        }
    }

    // A hold head whose tail never appears still demands a step.
    for (column, open) in open_holds.into_iter().enumerate() {
        if let Some((beat, quant, _)) = open {
            notes.push(Note {
                beat,
                column,
                quant,
                kind: NoteKind::Tap,
            });
        }
    }

    notes.sort_by(|a, b| a.beat.0.total_cmp(&b.beat.0).then(a.column.cmp(&b.column)));
    Some((columns, notes))
}

/// The standard note values, quarters through 64ths, as notes-per-measure.
const QUANT_LADDER: [u32; 8] = [4, 8, 12, 16, 24, 32, 48, 64];

/// The note value of row `row_index` in a measure of `row_count` rows: the
/// coarsest standard grid the row lands on. The row's exact position is the
/// reduced fraction `row_index / row_count`; the note value is the first
/// ladder entry that denominator divides (a row at 1/6 of a measure sits on
/// the 12th-note grid). Positions off every standard grid keep their exact
/// denominator.
fn quantization(row_index: usize, row_count: usize) -> u32 {
    let denominator = (row_count / gcd(row_index, row_count)) as u32;
    QUANT_LADDER
        .into_iter()
        .find(|quant| quant % denominator == 0)
        .unwrap_or(denominator)
}

fn gcd(a: usize, b: usize) -> usize {
    if a == 0 { b } else { gcd(b % a, a) }
}

fn is_note_char(char: u8) -> bool {
    matches!(char, b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z')
}
