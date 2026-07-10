//! Renders note-field animation scenarios to mp4 files, so sprite rendering
//! can be reviewed frame by frame without playing the game.
//!
//! ```text
//! cargo run --bin render_note all --skin ddrextreme_default --bpm 125
//! cargo run --bin render_note hold_quant_16
//! cargo run --bin render_note --list
//! ```

use clap::Parser;
use rhythm::dev::launcher;
use rhythm::dev::note_scenarios::scenario_names;
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Render note-field animation scenarios to mp4 files")]
struct Cli {
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
}

fn main() {
    let cli = Cli::parse();
    if cli.list {
        for name in scenario_names() {
            println!("{name}");
        }
        return;
    }
    let matches = scenario_names()
        .into_iter()
        .any(|name| cli.filter == "all" || name.contains(&cli.filter));
    if !matches {
        eprintln!(
            "no scenario matches {:?}; use --list to see all",
            cli.filter
        );
        std::process::exit(1);
    }

    std::fs::create_dir_all(&cli.out).expect("failed to create the output directory");
    let out = std::fs::canonicalize(&cli.out).expect("output directory resolves");
    let mut args = vec![
        "--render-note".to_string(),
        cli.filter,
        "--perspective".to_string(),
        cli.perspective,
        "--bpm".to_string(),
        cli.bpm.to_string(),
        "--out".to_string(),
        out.to_string_lossy().into_owned(),
        "--fps".to_string(),
        cli.fps.to_string(),
    ];
    if let Some(skin) = cli.skin {
        args.push("--skin".to_string());
        args.push(skin);
    }

    launcher::build_extension(false);
    let sandbox = std::env::temp_dir().join("rhythm-render");
    let status = launcher::run_game(&args, Some(&sandbox));
    std::process::exit(status.code().unwrap_or(1));
}
