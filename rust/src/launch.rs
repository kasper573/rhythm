//! Generic launch directives, read from the user args after `--` on the
//! command line: deep-link any scene with its params, automate an input,
//! record frame times, or quit on a timer. Small parameterized affordances
//! that debugging and external tooling compose freely — the game neither
//! knows nor cares who is asking.
//!
//! ```text
//! --scene <name> [--stepfile <group>/<title>] [--difficulty <rank>]
//!                [--mode <singles|doubles|versus>]
//! --scenario <name> --skin <name> --bpm <f64> --perspective <p>   (note-demo)
//! --pulse <action>[:<seconds>]   tap an action on a cycle
//! --hold <action>                hold an action down
//! --frame-report <file>          write per-frame seconds as JSON on exit
//! --quit-after-seconds <s>       quit once this much time has passed
//! --profile <file>               stream a chrome trace (profile builds)
//! ```

use crate::core::config::{RowOutcome, config};
use crate::core::input::GameAction;
use crate::core::library::{StepfileEntry, StepfileId, StepfileLibrary, library};
use crate::core::player::PlayMode;
use crate::core::settings::Settings;
use crate::core::units::Seconds;
use crate::game::Game;
use crate::nodes::stepfile_player::StageResults;
use crate::scenes::GameScene;
use crate::scenes::note_demo::NoteDemoParams;
use crate::scenes::play::{PlayerChart, SelectedStepfile};
use crate::scenes::score::{PlayerResult, ScoreResults};
use godot::classes::{INode, Input, InputEventKey, Node, Os};
use godot::prelude::*;
use std::path::PathBuf;
use std::str::FromStr;

/// Applies every launch directive to the freshly booted game and returns
/// the scene to enter.
pub fn boot(game: &mut Game) -> GameScene {
    let args: Vec<String> = Os::singleton()
        .get_cmdline_user_args()
        .as_slice()
        .iter()
        .map(|arg| arg.to_string())
        .collect();
    let value = |flag: &str| {
        args.iter()
            .position(|arg| arg == flag)
            .and_then(|index| args.get(index + 1))
            .cloned()
    };

    if let Some(path) = value("--profile") {
        crate::profiling::enable(path.into());
    }
    if let Some(mode) = value("--mode") {
        use strum::IntoEnumIterator;
        let mode = PlayMode::iter()
            .find(|candidate| <&str>::from(*candidate).eq_ignore_ascii_case(&mode))
            .unwrap_or_else(|| panic!("unknown --mode {mode:?}; one of: singles, doubles, versus"));
        game.set_play_mode(mode);
    }
    if let Some(rank) = value("--difficulty") {
        let rank: u8 = rank.parse().expect("--difficulty must be a rank number");
        let mut preferred = game.preferred_difficulty();
        preferred.p1 = rank;
        preferred.p2 = rank;
        game.set_preferred_difficulty(preferred);
    }

    let pulse = value("--pulse").map(parse_cycle);
    let hold = value("--hold").map(|action| parse_action(&action));
    let report = value("--frame-report").map(PathBuf::from);
    let quit_after: Option<f64> = value("--quit-after-seconds")
        .map(|seconds| seconds.parse().expect("--quit-after-seconds is seconds"));
    if pulse.is_some() || hold.is_some() || report.is_some() || quit_after.is_some() {
        let mut node = LaunchRig::new_alloc();
        {
            let mut rig = node.bind_mut();
            rig.pulse = pulse;
            rig.hold = hold;
            rig.report = report;
            rig.quit_after = quit_after;
        }
        game.base_mut().add_child(&node);
    }

    let Some(scene) = value("--scene") else {
        return GameScene::MainMenu;
    };
    let scene = GameScene::from_str(&scene)
        .unwrap_or_else(|_| panic!("unknown --scene {scene:?}; one of: {}", scene_names()));
    match scene {
        GameScene::Play => {
            let (id, entry) = resolve_stepfile(library(), value("--stepfile"));
            game.set_selected_stepfile(selection(game.play_mode(), game, id, entry));
        }
        GameScene::Score => {
            let (id, entry) = resolve_stepfile(library(), value("--stepfile"));
            game.set_score_results(sample_results(game.play_mode(), game, id, entry));
        }
        GameScene::NoteDemo => {
            game.set_note_demo(NoteDemoParams {
                scenario: value("--scenario"),
                skin: value("--skin"),
                perspective: value("--perspective").unwrap_or_else(|| "None".to_string()),
                bpm: value("--bpm").map_or(120.0, |bpm| bpm.parse().expect("--bpm is a number")),
            });
        }
        _ => {}
    }
    scene
}

fn scene_names() -> String {
    use strum::IntoEnumIterator;
    GameScene::iter()
        .map(|scene| <&str>::from(scene).to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// `<group-substring>/<exact title>`; the config's wheel default when
/// omitted.
fn resolve_stepfile(
    library: &StepfileLibrary,
    spec: Option<String>,
) -> (StepfileId, &StepfileEntry) {
    let spec = spec.unwrap_or_else(|| {
        let (group, title) = &config().wheel_default;
        format!("{group}/{title}")
    });
    let (group_spec, title) = spec.split_once('/').expect("--stepfile is <group>/<title>");
    for (group_index, group) in library.groups.iter().enumerate() {
        if !group.name.contains(group_spec) {
            continue;
        }
        for (stepfile_index, entry) in group.stepfiles.iter().enumerate() {
            if entry.display_title() == title {
                return (
                    StepfileId {
                        group: group_index,
                        stepfile: stepfile_index,
                    },
                    entry,
                );
            }
        }
    }
    panic!("stepfile {spec:?} is not in the library");
}

/// What the wheel would insert when starting `entry` in the current mode.
fn selection(
    mode: PlayMode,
    game: &Game,
    id: StepfileId,
    entry: &StepfileEntry,
) -> SelectedStepfile {
    let preferred = game.preferred_difficulty();
    let charts = mode
        .players()
        .iter()
        .map(|player| PlayerChart {
            player: *player,
            chart: entry
                .stepfile
                .closest_chart(&mode.steps_type(), preferred[*player])
                .unwrap_or_else(|| panic!("{:?} has no {mode:?} charts", entry.display_title())),
        })
        .collect();
    SelectedStepfile { id, charts }
}

/// What a finished session would insert: a deterministic spread of grades
/// over the chart, so the score scene has something representative to show.
fn sample_results(
    mode: PlayMode,
    game: &Game,
    id: StepfileId,
    entry: &StepfileEntry,
) -> ScoreResults {
    let selection = selection(mode, game, id, entry);
    let players = selection
        .charts
        .iter()
        .map(|picked| {
            let chart = &entry.stepfile.charts[picked.chart];
            let stats = chart.stats();
            let rows_total = chart.rows.len() as u32;
            let outcomes = (0..rows_total)
                .map(|index| match index % 10 {
                    9 => RowOutcome::Miss,
                    step => RowOutcome::Hit {
                        error: Seconds((step as f64 - 4.0) * 0.005),
                    },
                })
                .collect();
            PlayerResult {
                chart: picked.chart,
                stage: StageResults {
                    player: picked.player,
                    failed: false,
                    outcomes,
                    rows_total,
                    max_combo: rows_total / 3,
                    holds_ok: stats.holds as u32,
                    holds_ng: 0,
                    holds_total: stats.holds as u32,
                    mines_exploded: 0,
                    mines_total: stats.mines as u32,
                },
            }
        })
        .collect();
    ScoreResults {
        id,
        title: entry.display_title(),
        players,
    }
}

fn parse_cycle(spec: String) -> (GameAction, f64) {
    match spec.split_once(':') {
        Some((action, seconds)) => (
            parse_action(action),
            seconds.parse().expect("--pulse interval is seconds"),
        ),
        None => (parse_action(&spec), 0.5),
    }
}

fn parse_action(name: &str) -> GameAction {
    serde_json::from_value(serde_json::Value::String(name.to_string()))
        .unwrap_or_else(|_| panic!("unknown action {name:?}"))
}

/// The launch directives that run for the whole session: input automation,
/// the frame report, and the quit timer.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct LaunchRig {
    pulse: Option<(GameAction, f64)>,
    hold: Option<GameAction>,
    held: bool,
    since: f64,
    report: Option<PathBuf>,
    frames: Vec<f64>,
    quit_after: Option<f64>,
    elapsed: f64,
    base: Base<Node>,
}

#[godot_api]
impl INode for LaunchRig {
    fn init(base: Base<Node>) -> LaunchRig {
        LaunchRig {
            pulse: None,
            hold: None,
            held: false,
            since: 0.0,
            report: None,
            frames: Vec::new(),
            quit_after: None,
            elapsed: 0.0,
            base,
        }
    }

    fn process(&mut self, delta: f64) {
        self.elapsed += delta;
        if self.report.is_some() {
            self.frames.push(delta);
        }
        if let Some(action) = self.hold
            && !self.held
        {
            self.held = true;
            press(action, true);
        }
        if let Some((action, interval)) = self.pulse {
            self.since += delta;
            if self.held {
                press(action, false);
                self.held = false;
            }
            if self.since >= interval {
                self.since -= interval;
                press(action, true);
                self.held = true;
            }
        }
        if self.quit_after.is_some_and(|after| self.elapsed >= after) {
            self.quit_after = None;
            self.base().get_tree().quit();
        }
    }

    fn exit_tree(&mut self) {
        if let Some(path) = self.report.take() {
            let report = serde_json::json!({
                "debug_build": cfg!(debug_assertions),
                "frames": self.frames,
            });
            std::fs::write(&path, report.to_string()).expect("failed to write the frame report");
        }
        crate::profiling::finish();
    }
}

/// Presses or releases an action's bound key through the real input
/// pipeline, exactly where hardware events land.
fn press(action: GameAction, pressed: bool) {
    let settings = Settings::singleton();
    let key = settings.bind().machine().keymap.key(action, config());
    let mut event = InputEventKey::new_gd();
    event.set_physical_keycode(key.0);
    event.set_pressed(pressed);
    Input::singleton().parse_input_event(&event);
}
