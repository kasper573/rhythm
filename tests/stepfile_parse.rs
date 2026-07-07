use rhythm::core::stepfile::{Difficulty, NoteKind, Stepfile, StepsType};
use rhythm::core::units::Beat;

const MINIMAL_SM: &str = r#"
#TITLE:Test Stepfile;
#ARTIST:Tester;
#MUSIC:test.mp3;
#OFFSET:-0.100;
#SAMPLESTART:9.5;
#SAMPLELENGTH:12;
#BPMS:0.000=120.000,8.000=240.000;
#STOPS:4.000=1.500;
#BGCHANGES:0.000=intro.avi=1.000=1=1=0;
//---------------dance-single - ----------------
#NOTES:
     dance-single:
     :
     Medium:
     5:
     0.1,0.2,0.3,0.4,0.5:
1000
0100
0010
0001
,  // measure two has eighths
10000000 // overlong row is truncated to the chart's columns
M000
2000
0000
0000
0000
3000
0000
"#;

#[test]
fn parses_minimal_stepfile() {
    let stepfile = Stepfile::parse(MINIMAL_SM).unwrap();
    assert_eq!(stepfile.title, "Test Stepfile");
    assert_eq!(stepfile.music.as_deref(), Some("test.mp3"));
    assert_eq!(stepfile.sample_start.0, 9.5);
    assert_eq!(stepfile.bg_changes.len(), 1);
    assert_eq!(stepfile.bg_changes[0].file, "intro.avi");

    assert_eq!(stepfile.charts.len(), 1);
    let chart = &stepfile.charts[0];
    assert_eq!(chart.steps_type, StepsType::DanceSingle);
    assert_eq!(chart.difficulty, Difficulty::Medium);
    assert_eq!(chart.meter, 5);
    assert_eq!(chart.columns, 4);

    // Measure one: four quarter notes on beats 0..4 walking the columns.
    assert_eq!(chart.notes[0].beat, Beat(0.0));
    assert_eq!(chart.notes[0].column, 0);
    assert_eq!(chart.notes[3].beat, Beat(3.0));
    assert_eq!(chart.notes[3].column, 3);

    // Measure two: eight eighth-note rows starting at beat 4.
    let tap = &chart.notes[4];
    assert_eq!(tap.beat, Beat(4.0));
    assert_eq!(tap.kind, NoteKind::Tap);
    let mine = &chart.notes[5];
    assert_eq!(mine.kind, NoteKind::Mine);
    assert_eq!(mine.beat, Beat(4.5));
    let hold = &chart.notes[6];
    assert_eq!(hold.beat, Beat(5.0));
    assert_eq!(hold.kind, NoteKind::Hold { end: Beat(7.0) });

    // Note values: on-beat rows are quarters even in subdivided measures;
    // the off-beat mine in an eight-row measure is an eighth.
    assert_eq!(chart.notes[0].quant, 4);
    assert_eq!(tap.quant, 4);
    assert_eq!(mine.quant, 8);
    assert_eq!(hold.quant, 4);

    // The stop delays the mapped time of everything past beat 4.
    let timing = &stepfile.timing;
    let quarter = 60.0 / 120.0;
    let expected = 5.0 * quarter + 1.5 + 0.1;
    assert!((timing.seconds_at_beat(Beat(5.0)).0 - expected).abs() < 1e-9);
}

#[test]
fn derives_note_values_from_measure_position() {
    // One measure of twelve rows: a triplet grid.
    let source = "#TITLE:Quants;\n#BPMS:0.000=120.000;\n#NOTES:\n dance-single:\n :\n Easy:\n 1:\n 0:\n1000\n0100\n0010\n1000\n0000\n0000\n1000\n0000\n0000\n1000\n0000\n0000\n;";
    let stepfile = rhythm::core::stepfile::Stepfile::parse(source).unwrap();
    let quants: Vec<u32> = stepfile.charts[0]
        .notes
        .iter()
        .map(|note| note.quant)
        .collect();
    // Rows 0/3/6/9 are quarters; row 1 is a 12th; row 2 sits at 1/6 of the
    // measure, whose coarsest standard grid is also the 12th-note grid.
    assert_eq!(quants, vec![4, 12, 12, 4, 4, 4]);
}

#[test]
fn rejects_stepfile_without_charts() {
    let source = "#TITLE:Empty;\n#BPMS:0.000=120.000;";
    assert!(Stepfile::parse(source).is_err());
}

#[test]
fn parses_every_stepfile_in_assets() {
    // The library convention: stepfiles/<group>/<stepfile>/*.sm
    let pattern = format!("{}/assets/stepfiles/*/*/*.sm", env!("CARGO_MANIFEST_DIR"));
    let paths: Vec<_> = glob::glob(&pattern)
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    assert!(!paths.is_empty(), "no stepfiles found under assets");

    let mut failures = Vec::new();
    for path in &paths {
        match Stepfile::load(path) {
            Err(error) => failures.push(format!("{}: {error}", path.display())),
            Ok(stepfile) => {
                if stepfile.title.is_empty() {
                    failures.push(format!("{}: empty title", path.display()));
                }
                let has_playable_chart = stepfile.charts.iter().any(|chart| {
                    chart.steps_type == StepsType::DanceSingle && !chart.notes.is_empty()
                });
                if !has_playable_chart {
                    failures.push(format!(
                        "{}: no dance-single chart with notes",
                        path.display()
                    ));
                }
                for chart in &stepfile.charts {
                    let mut previous = f64::NEG_INFINITY;
                    for note in &chart.notes {
                        let seconds = stepfile.timing.seconds_at_beat(note.beat).0;
                        if !seconds.is_finite() || seconds < previous {
                            failures.push(format!(
                                "{}: non-monotonic note time at {}",
                                path.display(),
                                note.beat
                            ));
                            break;
                        }
                        previous = seconds;
                    }
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} stepfiles failed:\n{}",
        failures.len(),
        paths.len(),
        failures.join("\n")
    );
}
