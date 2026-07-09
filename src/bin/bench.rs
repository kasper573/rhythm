//! Frame-rate benchmarks: boots the real game, drives it into a preset
//! scenario, measures five seconds of frame times, and reports fps
//! percentiles. `--profile` additionally captures a chrome trace
//! (requires building with `--features profile`).

use bevy::input::InputSystems;
use bevy::prelude::*;
use clap::{Parser, ValueEnum};
use rhythm::core::config::RowOutcome;
use rhythm::core::input::NavPulseSystems;
use rhythm::core::library::{StepfileEntry, StepfileId, StepfileLibrary};
use rhythm::core::platform::{AssetEntry, AudioChannel, Platform, SoundOptions, VideoSource};
use rhythm::core::units::Seconds;
use rhythm::native::NativePlatform;
use rhythm::scenes::file_player::{PlayResult, ScoreResults};
use rhythm::scenes::file_select::SelectedStepfile;
use rhythm::scenes::{GameScene, SceneFade};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The stepfile the file player and score scenarios run on.
const BENCH_GROUP: &str = "EXTREME";
const BENCH_TITLE: &str = "Dance Dance Revolution";

const BOOT_SECONDS: f64 = 1.5;
const SETTLE_SECONDS: f64 = 2.0;
/// The file player needs its lead-in to pass so music and arrows flow.
const PLAYER_SETTLE_SECONDS: f64 = 4.5;
const MEASURE_SECONDS: f64 = 5.0;
const TAP_INTERVAL_SECONDS: f64 = 0.5;

#[derive(Parser)]
struct Cli {
    /// The preset scenario to measure; all of them in sequence when
    /// omitted.
    #[arg(value_enum)]
    scenario: Option<Scenario>,
    /// Also capture a chrome trace into the working directory (requires
    /// a build with `--features profile`).
    #[arg(long)]
    profile: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Scenario {
    /// Idle on the main menu.
    MainMenu,
    /// Idle on the file select wheel.
    FileSelect,
    /// Tapping right twice per second on the wheel.
    FileSelectTap,
    /// Holding right, scrolling the wheel at full speed.
    FileSelectHold,
    /// Playing a stepfile, hands off.
    FilePlayer,
    /// Idle on the score screen.
    Score,
}

impl Scenario {
    /// The CLI spelling, reused for output and trace file names.
    fn name(self) -> String {
        self.to_possible_value()
            .expect("every scenario is a CLI value")
            .get_name()
            .to_string()
    }

    fn target_scene(self) -> GameScene {
        match self {
            Scenario::MainMenu => GameScene::MainMenu,
            Scenario::FileSelect | Scenario::FileSelectTap | Scenario::FileSelectHold => {
                GameScene::FileSelect
            }
            Scenario::FilePlayer => GameScene::FilePlayer,
            Scenario::Score => GameScene::Score,
        }
    }

    fn settle_seconds(self) -> f64 {
        match self {
            Scenario::FilePlayer => PLAYER_SETTLE_SECONDS,
            _ => SETTLE_SECONDS,
        }
    }
}

fn main() {
    let cli = Cli::parse();
    let queue: Vec<Scenario> = match cli.scenario {
        Some(scenario) => vec![scenario],
        None => Scenario::value_variants().to_vec(),
    };
    if cli.profile {
        let trace = match cli.scenario {
            Some(scenario) => format!("trace-{}.json", scenario.name()),
            None => "trace-all.json".to_string(),
        };
        println!("{}", serde_json::json!({ "trace_file": trace }));
        rhythm::profiling::enable(PathBuf::from(trace));
    }
    let mut app = rhythm::app(BenchPlatform::default());
    // The game throttles unfocused windows to spare idle machines; the
    // bench window is rarely focused and must never be throttled.
    app.insert_resource(bevy::winit::WinitSettings {
        focused_mode: bevy::winit::UpdateMode::Continuous,
        unfocused_mode: bevy::winit::UpdateMode::Continuous,
    });
    app.insert_resource(Bench {
        queue,
        current: 0,
        phase: Phase::Boot,
        in_phase: 0.0,
        frames: Vec::new(),
        tap: TapCycle::default(),
    })
    .add_systems(
        PreUpdate,
        synthesize_input.after(InputSystems).before(NavPulseSystems),
    )
    .add_systems(Update, drive_benchmark);
    app.run();
    rhythm::profiling::finish();
}

#[derive(Resource)]
struct Bench {
    queue: Vec<Scenario>,
    current: usize,
    phase: Phase,
    /// Seconds spent in the current phase.
    in_phase: f64,
    /// Measured frame times.
    frames: Vec<f64>,
    tap: TapCycle,
}

impl Bench {
    fn scenario(&self) -> Scenario {
        self.queue[self.current]
    }
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

fn drive_benchmark(
    time: Res<Time<Real>>,
    mut bench: ResMut<Bench>,
    mut fade: ResMut<SceneFade>,
    scene: Res<State<GameScene>>,
    library: Res<StepfileLibrary>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let delta = time.delta_secs_f64();
    bench.in_phase += delta;
    match bench.phase {
        Phase::Boot => {
            if bench.in_phase < BOOT_SECONDS {
                return;
            }
            setup_scenario(
                bench.scenario(),
                scene.get(),
                &library,
                &mut fade,
                &mut commands,
            );
            bench.phase = Phase::Settle;
            bench.in_phase = 0.0;
        }
        Phase::Settle => {
            if bench.in_phase < bench.scenario().settle_seconds() {
                return;
            }
            assert!(
                *scene.get() == bench.scenario().target_scene(),
                "scenario expected {:?}, but the game is in {:?}",
                bench.scenario().target_scene(),
                scene.get()
            );
            info!(scenario = bench.scenario().name(), "bench measure start");
            bench.phase = Phase::Measure;
            bench.in_phase = 0.0;
            bench.frames.clear();
        }
        Phase::Measure => {
            bench.frames.push(delta);
            if bench.in_phase < MEASURE_SECONDS {
                return;
            }
            info!(scenario = bench.scenario().name(), "bench measure end");
            report(&bench);
            if bench.current + 1 < bench.queue.len() {
                bench.current += 1;
                setup_scenario(
                    bench.scenario(),
                    scene.get(),
                    &library,
                    &mut fade,
                    &mut commands,
                );
                bench.phase = Phase::Settle;
                bench.in_phase = 0.0;
            } else {
                exit.write(AppExit::Success);
            }
        }
    }
}

/// Puts the game where the scenario runs: what the menus and the wheel
/// would insert, then the game's own scene transition. Scenarios that
/// share the running scene keep it.
fn setup_scenario(
    scenario: Scenario,
    current: &GameScene,
    library: &StepfileLibrary,
    fade: &mut SceneFade,
    commands: &mut Commands,
) {
    let target = scenario.target_scene();
    if *current == target {
        return;
    }
    match target {
        GameScene::FilePlayer => {
            commands.insert_resource(bench_selection(library));
        }
        GameScene::Score => {
            commands.insert_resource(bench_score(library));
        }
        _ => {}
    }
    fade.begin(target);
}

/// Writes synthetic key state between bevy's input update and the pulse
/// emitter, exactly where real key events land.
fn synthesize_input(
    time: Res<Time<Real>>,
    mut bench: ResMut<Bench>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
) {
    if bench.phase == Phase::Boot {
        return;
    }
    match bench.scenario() {
        Scenario::FileSelectHold => keys.press(KeyCode::ArrowRight),
        Scenario::FileSelectTap => {
            bench.tap.since += time.delta_secs_f64();
            if bench.tap.down {
                keys.release(KeyCode::ArrowRight);
                bench.tap.down = false;
            }
            if bench.tap.since >= TAP_INTERVAL_SECONDS {
                bench.tap.since -= TAP_INTERVAL_SECONDS;
                keys.press(KeyCode::ArrowRight);
                bench.tap.down = true;
            }
        }
        _ => {
            if keys.pressed(KeyCode::ArrowRight) {
                keys.release(KeyCode::ArrowRight);
            }
        }
    }
}

/// One JSON object per scenario on stdout, machine- and LLM-readable.
fn report(bench: &Bench) {
    let mut sorted = bench.frames.clone();
    sorted.sort_by(f64::total_cmp);
    let fps_at = |percentile: f64| {
        let index = ((sorted.len() - 1) as f64 * percentile / 100.0).round() as usize;
        (1.0 / sorted[index] * 10.0).round() / 10.0
    };
    let total: f64 = sorted.iter().sum();
    println!(
        "{}",
        serde_json::json!({
            "scenario": bench.scenario().name(),
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

/// What the wheel would insert when starting the bench stepfile.
fn bench_selection(library: &StepfileLibrary) -> SelectedStepfile {
    let (id, entry) = bench_stepfile(library);
    let preferred = entry
        .stepfile
        .preferred_chart()
        .expect("the bench stepfile has charts");
    let chart = entry
        .stepfile
        .charts
        .iter()
        .position(|chart| std::ptr::eq(chart, preferred))
        .expect("the preferred chart comes from the same list");
    SelectedStepfile { id, chart }
}

/// What a finished session would insert: a deterministic spread of grades
/// over the bench stepfile's chart.
fn bench_score(library: &StepfileLibrary) -> ScoreResults {
    let (id, entry) = bench_stepfile(library);
    let chart = entry
        .stepfile
        .preferred_chart()
        .expect("the bench stepfile has charts");
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
        result: PlayResult::Cleared,
        difficulty: chart.difficulty.clone(),
        outcomes,
        rows_total,
        max_combo: rows_total / 3,
        holds_ok: stats.holds as u32,
        holds_ng: 0,
        holds_total: stats.holds as u32,
        mines_exploded: 0,
        mines_total: stats.mines as u32,
    }
}

/// The real desktop platform with user data sandboxed to a scratch
/// directory wiped per run, so benchmarks always start from default
/// settings and never touch the player's files.
struct BenchPlatform {
    inner: NativePlatform,
    user_data: PathBuf,
}

impl Default for BenchPlatform {
    fn default() -> Self {
        let user_data = std::env::temp_dir().join("rhythm-bench");
        let _ = std::fs::remove_dir_all(&user_data);
        BenchPlatform {
            inner: NativePlatform,
            user_data,
        }
    }
}

impl Platform for BenchPlatform {
    fn asset_root(&self) -> PathBuf {
        self.inner.asset_root()
    }

    fn read_asset(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.inner.read_asset(path)
    }

    fn list_asset_dir(&self, dir: &Path) -> Vec<AssetEntry> {
        self.inner.list_asset_dir(dir)
    }

    fn asset_exists(&self, path: &Path) -> bool {
        self.inner.asset_exists(path)
    }

    fn load_user_data(&self, file_name: &str) -> io::Result<Option<String>> {
        let path = self.user_data.join(file_name);
        if !path.exists() {
            return Ok(None);
        }
        std::fs::read_to_string(&path).map(Some)
    }

    fn save_user_data(&self, file_name: &str, json: &str) -> io::Result<()> {
        std::fs::create_dir_all(&self.user_data)?;
        std::fs::write(self.user_data.join(file_name), json)
    }

    fn user_data_location(&self, file_name: &str) -> String {
        self.user_data.join(file_name).display().to_string()
    }

    fn open_video(&self, path: &Path, looping: bool) -> Result<Box<dyn VideoSource>, String> {
        self.inner.open_video(path, looping)
    }

    fn open_audio(
        &self,
        bytes: Arc<[u8]>,
        options: SoundOptions,
    ) -> Result<Box<dyn AudioChannel>, String> {
        self.inner.open_audio(bytes, options)
    }
}
