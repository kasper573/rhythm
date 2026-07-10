//! Frame-rate benchmarks: drives the real game into a preset scenario,
//! measures five seconds of frame times, and reports fps percentiles as
//! one JSON object per scenario on stdout.

use crate::core::config::{RowOutcome, config};
use crate::core::input::GameAction;
use crate::core::library::{StepfileEntry, StepfileId, StepfileLibrary, library};
use crate::core::player::PlayerId;
use crate::core::stepfile::{Difficulty, StepsType};
use crate::core::units::Seconds;
use crate::game::Game;
use crate::nodes::stepfile_player::StageResults;
use crate::scenes::GameScene;
use crate::scenes::play::{PlayerChart, SelectedStepfile};
use crate::scenes::score::{PlayerResult, ScoreResults};
use godot::classes::{INode, Input, InputEventKey, Node};
use godot::prelude::*;
use strum::{EnumIter, IntoEnumIterator, IntoStaticStr};

/// The stepfile the play and score scenarios run on.
const BENCH_GROUP: &str = "EXTREME";
const BENCH_TITLE: &str = "Dance Dance Revolution";

const BOOT_SECONDS: f64 = 1.5;
const SETTLE_SECONDS: f64 = 2.0;
/// The play scene needs its lead-in to pass so music and arrows flow.
const PLAYER_SETTLE_SECONDS: f64 = 4.5;
const MEASURE_SECONDS: f64 = 5.0;
const TAP_INTERVAL_SECONDS: f64 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum Scenario {
    /// Idle on the main menu.
    MainMenu,
    /// Idle on the wheel.
    Wheel,
    /// Tapping right twice per second on the wheel.
    WheelTap,
    /// Holding right, scrolling the wheel at full speed.
    WheelHold,
    /// Playing a stepfile, hands off.
    Play,
    /// Idle on the score screen.
    Score,
}

/// The CLI spellings, for the launcher's validation and help.
pub fn scenario_names() -> Vec<&'static str> {
    Scenario::iter().map(<&str>::from).collect()
}

impl Scenario {
    fn name(self) -> &'static str {
        self.into()
    }

    fn target_scene(self) -> GameScene {
        match self {
            Scenario::MainMenu => GameScene::MainMenu,
            Scenario::Wheel | Scenario::WheelTap | Scenario::WheelHold => GameScene::Wheel,
            Scenario::Play => GameScene::Play,
            Scenario::Score => GameScene::Score,
        }
    }

    fn settle_seconds(self) -> f64 {
        match self {
            Scenario::Play => PLAYER_SETTLE_SECONDS,
            _ => SETTLE_SECONDS,
        }
    }
}

/// Boots the driver onto the game: the main menu comes up as usual and the
/// driver walks the scenario queue from there.
pub(super) fn start(game: &mut Game, scenario: &str) {
    let queue: Vec<Scenario> = if scenario == "all" {
        Scenario::iter().collect()
    } else {
        vec![
            Scenario::iter()
                .find(|candidate| candidate.name() == scenario)
                .unwrap_or_else(|| panic!("unknown bench scenario {scenario:?}")),
        ]
    };
    game.change_scene(GameScene::MainMenu);
    let mut driver = BenchDriver::new_alloc();
    driver.bind_mut().queue = queue;
    game.base_mut().add_child(&driver);
}

#[derive(GodotClass)]
#[class(base=Node)]
struct BenchDriver {
    queue: Vec<Scenario>,
    current: usize,
    phase: Phase,
    /// Seconds spent in the current phase.
    in_phase: f64,
    /// Measured frame times.
    frames: Vec<f64>,
    tap: TapCycle,
    base: Base<Node>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Waiting out startup and the boot fade before touching anything.
    Boot,
    /// The scenario is set up; waiting for transitions and loads to pass.
    Settle,
    Measure,
}

#[derive(Default)]
struct TapCycle {
    since: f64,
    down: bool,
}

impl BenchDriver {
    fn scenario(&self) -> Scenario {
        self.queue[self.current]
    }

    /// Puts the game where the scenario runs: what the menus and the wheel
    /// would insert, then the game's own scene transition. Scenarios that
    /// share the running scene keep it.
    fn setup_scenario(&self) {
        let scenario = self.scenario();
        let mut game = Game::singleton();
        let mut game = game.bind_mut();
        if game.scene() == scenario.target_scene() {
            return;
        }
        match scenario.target_scene() {
            GameScene::Play => {
                game.set_selected_stepfile(bench_selection(library()));
            }
            GameScene::Score => {
                game.set_score_results(bench_score(library()));
            }
            _ => {}
        }
        game.change_scene(scenario.target_scene());
    }

    /// Synthesizes the scenario's key state through the real input path.
    fn synthesize_input(&mut self, delta: f64) {
        if self.phase == Phase::Boot {
            return;
        }
        match self.scenario() {
            Scenario::WheelHold => {
                if !self.tap.down {
                    self.tap.down = true;
                    press_scroll_key(true);
                }
            }
            Scenario::WheelTap => {
                self.tap.since += delta;
                if self.tap.down {
                    press_scroll_key(false);
                    self.tap.down = false;
                }
                if self.tap.since >= TAP_INTERVAL_SECONDS {
                    self.tap.since -= TAP_INTERVAL_SECONDS;
                    press_scroll_key(true);
                    self.tap.down = true;
                }
            }
            _ => {
                if self.tap.down {
                    self.tap.down = false;
                    press_scroll_key(false);
                }
            }
        }
    }

    /// One JSON object per scenario on stdout, machine- and LLM-readable.
    fn report(&self) {
        let mut sorted = self.frames.clone();
        sorted.sort_by(f64::total_cmp);
        let fps_at = |percentile: f64| {
            let index = ((sorted.len() - 1) as f64 * percentile / 100.0).round() as usize;
            (1.0 / sorted[index] * 10.0).round() / 10.0
        };
        let total: f64 = sorted.iter().sum();
        println!(
            "{}",
            serde_json::json!({
                "scenario": self.scenario().name(),
                "frames": sorted.len(),
                "seconds": (total * 100.0).round() / 100.0,
                "fps": {
                    "p50": fps_at(50.0),
                    "p95": fps_at(95.0),
                    "p99": fps_at(99.0),
                },
                "debug_build": cfg!(debug_assertions),
            })
        );
    }
}

#[godot_api]
impl INode for BenchDriver {
    fn init(base: Base<Node>) -> BenchDriver {
        BenchDriver {
            queue: Vec::new(),
            current: 0,
            phase: Phase::Boot,
            in_phase: 0.0,
            frames: Vec::new(),
            tap: TapCycle::default(),
            base,
        }
    }

    fn process(&mut self, delta: f64) {
        self.synthesize_input(delta);
        self.in_phase += delta;
        match self.phase {
            Phase::Boot => {
                if self.in_phase < BOOT_SECONDS {
                    return;
                }
                self.setup_scenario();
                self.phase = Phase::Settle;
                self.in_phase = 0.0;
            }
            Phase::Settle => {
                if self.in_phase < self.scenario().settle_seconds() {
                    return;
                }
                let scene = Game::singleton().bind().scene();
                assert!(
                    scene == self.scenario().target_scene(),
                    "scenario expected {:?}, but the game is in {scene:?}",
                    self.scenario().target_scene(),
                );
                self.phase = Phase::Measure;
                self.in_phase = 0.0;
                self.frames.clear();
            }
            Phase::Measure => {
                self.frames.push(delta);
                if self.in_phase < MEASURE_SECONDS {
                    return;
                }
                self.report();
                if self.current + 1 < self.queue.len() {
                    self.current += 1;
                    self.setup_scenario();
                    self.phase = Phase::Settle;
                    self.in_phase = 0.0;
                } else {
                    super::profiling::finish();
                    self.base().get_tree().quit();
                }
            }
        }
    }
}

/// Presses or releases the wheel-scroll key (P1's right step) through the
/// real input pipeline, exactly where hardware events land.
fn press_scroll_key(pressed: bool) {
    let key = config()
        .defaults
        .keymap
        .binding(GameAction::P1Right)
        .expect("validated: defaults.keymap binds every action");
    let mut event = InputEventKey::new_gd();
    event.set_physical_keycode(key.0);
    event.set_pressed(pressed);
    Input::singleton().parse_input_event(&event);
}

fn bench_stepfile(library: &StepfileLibrary) -> (StepfileId, &StepfileEntry) {
    for (group_index, group) in library.groups.iter().enumerate() {
        if !group.name.contains(BENCH_GROUP) {
            continue;
        }
        for (stepfile_index, entry) in group.stepfiles.iter().enumerate() {
            if entry.display_title() == BENCH_TITLE {
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
    panic!("bench stepfile {BENCH_GROUP:?}/{BENCH_TITLE:?} is not in the library");
}

/// What the wheel would insert when starting the bench stepfile in singles.
fn bench_selection(library: &StepfileLibrary) -> SelectedStepfile {
    let (id, entry) = bench_stepfile(library);
    SelectedStepfile {
        id,
        charts: vec![PlayerChart {
            player: PlayerId::P1,
            chart: bench_chart_index(entry),
        }],
    }
}

/// The chart the bench plays: what the wheel would pick at the default
/// difficulty preference.
fn bench_chart_index(entry: &StepfileEntry) -> usize {
    entry
        .stepfile
        .closest_chart(&StepsType::DanceSingle, Difficulty::Medium.rank())
        .expect("the bench stepfile has singles charts")
}

/// What a finished session would insert: a deterministic spread of grades
/// over the bench stepfile's chart.
fn bench_score(library: &StepfileLibrary) -> ScoreResults {
    let (id, entry) = bench_stepfile(library);
    let chart_index = bench_chart_index(entry);
    let chart = &entry.stepfile.charts[chart_index];
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
    ScoreResults {
        id,
        title: entry.display_title(),
        players: vec![PlayerResult {
            chart: chart_index,
            stage: StageResults {
                player: PlayerId::P1,
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
        }],
    }
}
