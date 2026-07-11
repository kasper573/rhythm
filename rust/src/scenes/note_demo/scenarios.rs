//! The note-field animation scenarios the demo scene plays: plain data,
//! one deterministic timeline per rendering behavior worth reviewing.

use crate::nodes::stepfile_player::note_field::HoldVisualState;

pub struct Scenario {
    pub name: String,
    pub notes: Vec<ScenarioNote>,
    pub mines: Vec<ScenarioMine>,
    pub script: Vec<(f64, ScriptAction)>,
    /// `(beat, bpm)` changes; empty means the CLI `--bpm` throughout.
    pub bpms: Vec<(f64, f64)>,
    /// `(beat, seconds)` stops.
    pub stops: Vec<(f64, f64)>,
}

pub struct ScenarioNote {
    pub beat: f64,
    pub column: usize,
    pub quant: u32,
    pub length_beats: Option<f64>,
    pub roll: bool,
}

pub struct ScenarioMine {
    pub beat: f64,
    pub column: usize,
}

/// A scripted stand-in for the gameplay systems that drive the field.
#[derive(Clone, Copy)]
pub enum ScriptAction {
    /// Set the render state of the scenario's i-th note's hold.
    Hold(usize, HoldVisualState),
    /// Apply the hold-OK fade to the i-th note's head.
    Fade(usize),
    /// Vanish the i-th note at the receptor: despawn it and play the arrow
    /// flash on its column, as grading does for taps.
    Vanish(usize),
    /// Press or release a receptor's panel.
    Press(usize, bool),
    /// Blow up the i-th mine.
    ExplodeMine(usize),
}

pub fn scenario_names() -> Vec<String> {
    scenario_matrix()
        .into_iter()
        .map(|scenario| scenario.name)
        .collect()
}

pub fn scenario_matrix() -> Vec<Scenario> {
    use HoldVisualState::{Dropped, Held, Ok, Released};
    use ScriptAction::{ExplodeMine, Fade, Hold, Press, Vanish};

    const QUANTS: [u32; 8] = [4, 8, 12, 16, 24, 32, 48, 64];
    let mut all = Vec::new();
    let mut add = |name: &str,
                   notes: Vec<ScenarioNote>,
                   mines: Vec<ScenarioMine>,
                   script: Vec<(f64, ScriptAction)>| {
        all.push(Scenario {
            name: name.to_string(),
            notes,
            mines,
            script,
            bpms: Vec::new(),
            stops: Vec::new(),
        });
    };

    for quant in QUANTS {
        add(
            &format!("single_quant_{quant}"),
            vec![note(0.0, 1, quant, None)],
            vec![],
            vec![],
        );
    }

    for quant in QUANTS {
        for (label, length) in [
            ("half_beat", 0.5),
            ("one_beat", 1.0),
            ("two_and_a_half_beats", 2.5),
        ] {
            add(
                &format!("hold_quant_{quant}_{label}"),
                vec![note(0.0, 1, quant, Some(length))],
                vec![],
                vec![],
            );
        }
    }

    add(
        "hold_held_to_ok",
        vec![note(0.0, 1, 4, Some(2.0))],
        vec![],
        vec![
            (0.0, Press(1, true)),
            (0.0, Hold(0, Held)),
            (2.0, Hold(0, Ok)),
            (2.0, Fade(0)),
            (2.0, Press(1, false)),
        ],
    );
    add(
        "hold_released_and_regrabbed",
        vec![note(0.0, 1, 4, Some(3.0))],
        vec![],
        vec![
            (0.0, Press(1, true)),
            (0.0, Hold(0, Held)),
            (1.0, Press(1, false)),
            (1.0, Hold(0, Released)),
            (1.75, Press(1, true)),
            (1.75, Hold(0, Held)),
            (3.0, Hold(0, Ok)),
            (3.0, Fade(0)),
            (3.0, Press(1, false)),
        ],
    );
    add(
        "hold_dropped_midway",
        vec![note(0.0, 1, 4, Some(3.0))],
        vec![],
        vec![
            (0.0, Press(1, true)),
            (0.0, Hold(0, Held)),
            (1.0, Press(1, false)),
            (1.0, Hold(0, Released)),
            (1.5, Hold(0, Dropped)),
        ],
    );
    add(
        "hold_head_missed",
        vec![note(0.0, 1, 4, Some(2.0))],
        vec![],
        vec![(0.5, Hold(0, Dropped))],
    );
    add("roll_two_beats", vec![roll(0.0, 1, 4, 2.0)], vec![], vec![]);
    add(
        "roll_held_to_ok",
        vec![roll(0.0, 1, 4, 2.0)],
        vec![],
        vec![
            (0.0, Press(1, true)),
            (0.0, Hold(0, Held)),
            (2.0, Hold(0, Ok)),
            (2.0, Fade(0)),
            (2.0, Press(1, false)),
        ],
    );
    add(
        "hold_chain_one_column",
        vec![
            note(0.0, 1, 4, Some(0.5)),
            note(1.0, 1, 4, Some(0.5)),
            note(2.0, 1, 4, Some(0.5)),
        ],
        vec![],
        vec![],
    );
    add(
        "hold_staircase",
        vec![
            note(0.0, 0, 4, Some(1.0)),
            note(0.5, 1, 8, Some(1.0)),
            note(1.0, 2, 4, Some(1.0)),
            note(1.5, 3, 8, Some(1.0)),
        ],
        vec![],
        vec![],
    );
    add(
        "jump_hold",
        vec![note(0.0, 1, 4, Some(2.0)), note(0.0, 2, 4, Some(2.0))],
        vec![],
        vec![],
    );

    add(
        "tap_vanish_at_receptor",
        vec![note(0.0, 1, 4, None)],
        vec![],
        vec![
            (0.0, Press(1, true)),
            (0.0, Vanish(0)),
            (0.4, Press(1, false)),
        ],
    );
    add(
        "jump",
        vec![note(0.0, 0, 4, None), note(0.0, 3, 4, None)],
        vec![],
        vec![],
    );
    add(
        "every_column",
        vec![
            note(0.0, 0, 4, None),
            note(1.0, 1, 4, None),
            note(2.0, 2, 4, None),
            note(3.0, 3, 4, None),
        ],
        vec![],
        vec![],
    );
    add(
        "stream_16ths",
        (0..16)
            .map(|i| {
                let quant = [4, 16, 8, 16][i % 4];
                note(i as f64 * 0.25, i % 4, quant, None)
            })
            .collect(),
        vec![],
        vec![],
    );

    add("mine", vec![], vec![mine(0.0, 1)], vec![]);
    add(
        "mine_row",
        vec![],
        (0..4).map(|column| mine(0.0, column)).collect(),
        vec![],
    );
    add(
        "mine_exploding",
        vec![],
        vec![mine(0.0, 1)],
        vec![
            (-0.5, Press(1, true)),
            (0.0, ExplodeMine(0)),
            (0.5, Press(1, false)),
        ],
    );

    add(
        "receptors_idle",
        vec![],
        vec![],
        vec![
            (1.0, Press(1, true)),
            (2.0, Press(1, false)),
            (3.0, Press(2, true)),
            (4.0, Press(2, false)),
        ],
    );

    // Tempo gimmicks: under Dynamic speed the spacing per beat must stay
    // uniform while the scroll rate doubles, and a stop must freeze the
    // field; under Constant speed the spacing itself changes instead.
    all.push(Scenario {
        name: "stream_bpm_change".to_string(),
        notes: (0..8).map(|i| note(i as f64, i % 4, 4, None)).collect(),
        mines: vec![],
        script: vec![],
        bpms: vec![(0.0, 125.0), (4.0, 250.0)],
        stops: vec![],
    });
    all.push(Scenario {
        name: "stream_stop".to_string(),
        notes: (0..8).map(|i| note(i as f64, i % 4, 4, None)).collect(),
        mines: vec![],
        script: vec![],
        bpms: vec![],
        stops: vec![(4.0, 1.0)],
    });

    all
}

fn note(beat: f64, column: usize, quant: u32, length_beats: Option<f64>) -> ScenarioNote {
    ScenarioNote {
        beat,
        column,
        quant,
        length_beats,
        roll: false,
    }
}

fn roll(beat: f64, column: usize, quant: u32, length_beats: f64) -> ScenarioNote {
    ScenarioNote {
        roll: true,
        ..note(beat, column, quant, Some(length_beats))
    }
}

fn mine(beat: f64, column: usize) -> ScenarioMine {
    ScenarioMine { beat, column }
}
