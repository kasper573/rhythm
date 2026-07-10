//! The dev command line — every tool that benches, renders, serves, or
//! exports the real game, one subcommand each. The launchers rebuild the
//! extension and boot the game via Godot, so a run always measures and
//! renders the code as it currently stands.
//!
//! ```text
//! cargo run -- bench [scenario] [--profile]
//! cargo run -- render-note [filter] [--skin ..] [--bpm ..] [--list]
//! cargo run -- render-grade
//! cargo run -- serve [--emit DIR] [--host ADDR] [--port PORT]
//! cargo run -- export <preset> <out>
//! ```

use clap::{Parser, Subcommand};
use rhythm::dev::note_scenarios::scenario_names;
use rhythm::dev::{launcher, serve};
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Rhythm's development tools")]
struct Cli {
    #[command(subcommand)]
    tool: Tool,
}

#[derive(Subcommand)]
enum Tool {
    /// Boot a preset scenario and report fps percentiles over five seconds
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

fn bench(scenario: Option<String>, profile: bool) {
    let names = rhythm::dev::bench_scenario_names();
    let scenario = scenario.unwrap_or_else(|| "all".to_string());
    assert!(
        scenario == "all" || names.contains(&scenario.as_str()),
        "unknown scenario {scenario:?}; one of: all, {}",
        names.join(", ")
    );

    let mut args = vec!["--bench".to_string(), scenario.clone()];
    if profile {
        let trace = match scenario.as_str() {
            "all" => "trace-all.json".to_string(),
            name => format!("trace-{name}.json"),
        };
        args.push("--profile".to_string());
        args.push(
            launcher::repo_root()
                .join(trace)
                .to_string_lossy()
                .into_owned(),
        );
    }

    launcher::build_extension(profile);
    let sandbox = std::env::temp_dir().join("rhythm-bench");
    let status = launcher::run_game(&args, Some(&sandbox));
    std::process::exit(status.code().unwrap_or(1));
}

fn render_note(
    filter: String,
    skin: Option<String>,
    perspective: String,
    bpm: f64,
    out: PathBuf,
    fps: u32,
    list: bool,
) {
    if list {
        for name in scenario_names() {
            println!("{name}");
        }
        return;
    }
    let matches = scenario_names()
        .into_iter()
        .any(|name| filter == "all" || name.contains(&filter));
    if !matches {
        eprintln!("no scenario matches {filter:?}; use --list to see all");
        std::process::exit(1);
    }

    std::fs::create_dir_all(&out).expect("failed to create the output directory");
    let out = std::fs::canonicalize(&out).expect("output directory resolves");
    let mut args = vec![
        "--render-note".to_string(),
        filter,
        "--perspective".to_string(),
        perspective,
        "--bpm".to_string(),
        bpm.to_string(),
        "--out".to_string(),
        out.to_string_lossy().into_owned(),
        "--fps".to_string(),
        fps.to_string(),
    ];
    if let Some(skin) = skin {
        args.push("--skin".to_string());
        args.push(skin);
    }

    launcher::build_extension(false);
    let sandbox = std::env::temp_dir().join("rhythm-render");
    let status = launcher::run_game(&args, Some(&sandbox));
    std::process::exit(status.code().unwrap_or(1));
}

fn render_grade() {
    std::fs::create_dir_all("out").expect("failed to create the output directory");
    let out = std::fs::canonicalize("out").expect("output directory resolves");
    launcher::build_extension(false);
    let sandbox = std::env::temp_dir().join("rhythm-render");
    let status = launcher::run_game(
        &[
            "--render-grade".to_string(),
            "--out".to_string(),
            out.to_string_lossy().into_owned(),
        ],
        Some(&sandbox),
    );
    std::process::exit(status.code().unwrap_or(1));
}
