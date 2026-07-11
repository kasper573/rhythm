//! The dev command line: benches, renders, serves, and exports the real
//! game by composing its generic launch directives — deep links, input
//! automation, frame reports — with Godot's own movie-maker capture. The
//! game knows nothing about any of this.
//!
//! ```text
//! cargo run -p tools -- bench [scenario] [--profile]
//! cargo run -p tools -- render-note [filter] [--skin ..] [--bpm ..] [--list]
//! cargo run -p tools -- render-grade
//! cargo run -p tools -- serve [--emit DIR] [--host ADDR] [--port PORT]
//! cargo run -p tools -- export <preset> <out>
//! ```

mod launcher;
mod serve;

use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(about = "Rhythm's development tools")]
struct Cli {
    #[command(subcommand)]
    tool: Tool,
}

#[derive(Subcommand)]
enum Tool {
    /// Deep-link a preset scenario and report fps percentiles
    Bench {
        /// The preset scenario to measure; all of them in sequence when
        /// omitted
        scenario: Option<String>,
        /// Also capture a chrome trace into the repository root
        #[arg(long)]
        profile: bool,
    },
    /// Render note-field animation scenarios to mp4 files
    RenderNote {
        /// "all", or a substring matched against scenario names
        #[arg(default_value = "all")]
        filter: String,
        /// Note skin to render; defaults to the game config's default
        #[arg(long)]
        skin: Option<String>,
        /// Lane camera perspective: None, Above, or Below
        #[arg(long, default_value = "None")]
        perspective: String,
        #[arg(long, default_value_t = 120.0)]
        bpm: f64,
        /// Output directory for the mp4 files
        #[arg(long, default_value = "out")]
        out: PathBuf,
        #[arg(long, default_value_t = 60)]
        fps: u32,
        /// List all scenario names and exit
        #[arg(long)]
        list: bool,
    },
    /// Render every grade text to out/grades.png for shader tuning
    RenderGrade,
    /// Build the web site and serve it (HTTPS off loopback), or emit it
    Serve {
        /// Write the complete static site (game, assets, index) to this
        /// directory and exit, instead of serving
        #[arg(long)]
        emit: Option<PathBuf>,
        /// Interface to bind; 0.0.0.0 exposes the server to the LAN
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 8080)]
        port: u16,
    },
    /// Export a shippable release build of one preset from
    /// godot/export_presets.cfg
    Export {
        /// Preset name from godot/export_presets.cfg
        preset: String,
        /// The exported game binary's path
        out: PathBuf,
    },
}

fn main() {
    match Cli::parse().tool {
        Tool::Bench { scenario, profile } => bench(scenario, profile),
        Tool::RenderNote {
            filter,
            skin,
            perspective,
            bpm,
            out,
            fps,
            list,
        } => render_note(filter, skin, perspective, bpm, out, fps, list),
        Tool::RenderGrade => render_grade(),
        Tool::Serve { emit, host, port } => serve::run(emit, &host, port),
        Tool::Export { preset, out } => {
            // The editor performing the export loads the debug library;
            // the exported game ships the release one.
            launcher::build_extension(false);
            launcher::build_extension_release();
            launcher::export_release(&preset, &out);
        }
    }
}

/// Boot padding and load time excluded from every measurement, and the
/// measured stretch itself.
const BENCH_SETTLE_SECONDS: f64 = 3.5;
const BENCH_PLAY_SETTLE_SECONDS: f64 = 6.0;
const BENCH_MEASURE_SECONDS: f64 = 5.0;
/// The stepfile the play and score scenarios run on.
const BENCH_STEPFILE: &str = "EXTREME/Dance Dance Revolution";

/// Each scenario is a deep link plus input automation; the game itself has
/// no notion of a benchmark.
const BENCH_SCENARIOS: [(&str, &[&str]); 6] = [
    ("main-menu", &["--scene", "main-menu"]),
    ("wheel", &["--scene", "wheel"]),
    ("wheel-tap", &["--scene", "wheel", "--pulse", "P1Right:0.5"]),
    ("wheel-hold", &["--scene", "wheel", "--hold", "P1Right"]),
    ("play", &["--scene", "play", "--stepfile", BENCH_STEPFILE]),
    ("score", &["--scene", "score", "--stepfile", BENCH_STEPFILE]),
];

fn bench(scenario: Option<String>, profile: bool) {
    let scenario = scenario.unwrap_or_else(|| "all".to_string());
    let queue: Vec<&(&str, &[&str])> = BENCH_SCENARIOS
        .iter()
        .filter(|(name, _)| scenario == "all" || *name == scenario)
        .collect();
    assert!(
        !queue.is_empty(),
        "unknown scenario {scenario:?}; one of: all, {}",
        BENCH_SCENARIOS.map(|(name, _)| name).join(", ")
    );

    launcher::build_extension(profile);
    for (name, link) in queue {
        let settle = match *name {
            "play" => BENCH_PLAY_SETTLE_SECONDS,
            _ => BENCH_SETTLE_SECONDS,
        };
        let report = std::env::temp_dir().join("rhythm-bench-report.json");
        let mut args: Vec<String> = link.iter().map(ToString::to_string).collect();
        args.extend([
            "--frame-report".to_string(),
            report.to_string_lossy().into_owned(),
            "--quit-after-seconds".to_string(),
            (settle + BENCH_MEASURE_SECONDS).to_string(),
        ]);
        if profile {
            args.push("--profile".to_string());
            args.push(
                launcher::repo_root()
                    .join(format!("trace-{name}.json"))
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        let sandbox = std::env::temp_dir().join("rhythm-bench");
        let status = launcher::run_game(&[], &args, Some(&sandbox));
        assert!(status.success(), "the game exited with a failure");
        report_percentiles(name, &report, settle);
    }
}

/// One JSON object per scenario on stdout, machine- and LLM-readable:
/// fps percentiles over the frames measured after the settle window.
fn report_percentiles(name: &str, report: &Path, settle: f64) {
    let raw = std::fs::read_to_string(report).expect("the game wrote a frame report");
    let report: serde_json::Value = serde_json::from_str(&raw).expect("frame report is JSON");
    let frames: Vec<f64> = report["frames"]
        .as_array()
        .expect("frame report holds frames")
        .iter()
        .map(|value| value.as_f64().expect("frame times are seconds"))
        .collect();
    let mut elapsed = 0.0;
    let mut sorted: Vec<f64> = frames
        .into_iter()
        .filter(|delta| {
            elapsed += delta;
            elapsed >= settle
        })
        .collect();
    assert!(!sorted.is_empty(), "no frames measured after the settle");
    sorted.sort_by(f64::total_cmp);
    let fps_at = |percentile: f64| {
        let index = ((sorted.len() - 1) as f64 * percentile / 100.0).round() as usize;
        (1.0 / sorted[index] * 10.0).round() / 10.0
    };
    let total: f64 = sorted.iter().sum();
    println!(
        "{}",
        serde_json::json!({
            "scenario": name,
            "frames": sorted.len(),
            "seconds": (total * 100.0).round() / 100.0,
            "fps": { "p50": fps_at(50.0), "p95": fps_at(95.0), "p99": fps_at(99.0) },
            "debug_build": report["debug_build"],
        })
    );
}

/// Frames Godot renders before a movie's content is trusted: the skin's
/// textures and pipelines warm up over the first captures.
const WARMUP_FRAMES: usize = 20;

fn render_note(
    filter: String,
    skin: Option<String>,
    perspective: String,
    bpm: f64,
    out: PathBuf,
    fps: u32,
    list: bool,
) {
    launcher::build_extension(false);
    let names = scenario_catalog();
    if list {
        for name in names {
            println!("{name}");
        }
        return;
    }
    let matching: Vec<String> = names
        .into_iter()
        .filter(|name| filter == "all" || name.contains(&filter))
        .collect();
    assert!(
        !matching.is_empty(),
        "no scenario matches {filter:?}; use --list to see all"
    );
    std::fs::create_dir_all(&out).expect("failed to create the output directory");

    for name in matching {
        let movie_dir = std::env::temp_dir().join("rhythm-movie");
        let _ = std::fs::remove_dir_all(&movie_dir);
        std::fs::create_dir_all(&movie_dir).expect("failed to create the movie directory");
        let mut args = vec![
            "--scene".to_string(),
            "note-demo".to_string(),
            "--scenario".to_string(),
            name.clone(),
            "--perspective".to_string(),
            perspective.clone(),
            "--bpm".to_string(),
            bpm.to_string(),
        ];
        if let Some(skin) = &skin {
            args.push("--skin".to_string());
            args.push(skin.clone());
        }
        let status = launcher::run_game(
            &movie_args(&movie_dir.join("f.png"), fps),
            &args,
            Some(&std::env::temp_dir().join("rhythm-render")),
        );
        assert!(status.success(), "the game exited with a failure");
        let target = out.join(format!("{name}.mp4"));
        encode_movie(&movie_dir, fps, &target);
        println!("wrote {}", target.display());
    }
}

fn render_grade() {
    launcher::build_extension(false);
    let movie_dir = std::env::temp_dir().join("rhythm-movie");
    let _ = std::fs::remove_dir_all(&movie_dir);
    std::fs::create_dir_all(&movie_dir).expect("failed to create the movie directory");
    let status = launcher::run_game(
        &movie_args(&movie_dir.join("f.png"), 60),
        &[
            "--scene".to_string(),
            "grade-sheet".to_string(),
            "--quit-after-seconds".to_string(),
            "0.8".to_string(),
        ],
        Some(&std::env::temp_dir().join("rhythm-render")),
    );
    assert!(status.success(), "the game exited with a failure");
    let last = movie_frames(&movie_dir)
        .pop()
        .expect("the movie captured frames");
    std::fs::create_dir_all("out").expect("failed to create the output directory");
    std::fs::copy(&last, "out/grades.png").expect("failed to copy the sheet");
    println!("wrote out/grades.png");
}

/// Godot's movie-maker flags: deterministic fixed-fps rendering into a
/// png frame sequence at the project resolution.
fn movie_args(movie: &Path, fps: u32) -> Vec<String> {
    vec![
        "--write-movie".to_string(),
        movie.to_string_lossy().into_owned(),
        "--fixed-fps".to_string(),
        fps.to_string(),
    ]
}

/// The movie's frames in order, warmup excluded.
fn movie_frames(movie_dir: &Path) -> Vec<PathBuf> {
    let mut frames: Vec<PathBuf> = std::fs::read_dir(movie_dir)
        .expect("the movie directory exists")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "png"))
        .collect();
    frames.sort();
    frames.split_off(WARMUP_FRAMES.min(frames.len().saturating_sub(1)))
}

/// Encodes the captured frames into an mp4 with the host's ffmpeg.
fn encode_movie(movie_dir: &Path, fps: u32, target: &PathBuf) {
    let frames = movie_frames(movie_dir);
    assert!(!frames.is_empty(), "the movie captured no frames");
    let list = movie_dir.join("frames.txt");
    let listing: String = frames
        .iter()
        .map(|frame| format!("file '{}'\n", frame.display()))
        .collect();
    std::fs::write(&list, listing).expect("failed to write the frame list");
    let status = std::process::Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-r", &fps.to_string()])
        .args(["-f", "concat", "-safe", "0", "-i"])
        .arg(&list)
        .args(["-c:v", "libx264", "-pix_fmt", "yuv420p"])
        .arg(target)
        .status()
        .expect("failed to run ffmpeg: install it on the PATH");
    assert!(status.success(), "ffmpeg failed");
}

/// The demo scene's catalog, queried from the game itself: launched with
/// no scenario it prints `scenario: <name>` lines and exits.
fn scenario_catalog() -> Vec<String> {
    let output = launcher::run_game_captured(
        &[],
        &["--scene".to_string(), "note-demo".to_string()],
        Some(&std::env::temp_dir().join("rhythm-render")),
    );
    let names: Vec<String> = output
        .lines()
        .filter_map(|line| line.strip_prefix("scenario: "))
        .map(str::to_string)
        .collect();
    assert!(!names.is_empty(), "the demo scene printed no catalog");
    names
}
