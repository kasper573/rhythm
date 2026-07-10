//! In-game development modes, activated by user args after `--` on the
//! godot command line: frame-rate benchmarks and headless-style renders.
//! The launcher binaries in `src/bin` build the extension, boot the real
//! game with these args, and collect the artifacts.

pub mod note_scenarios;
pub mod profiling;

mod bench;
mod render_grade;
mod render_note;

#[cfg(not(target_arch = "wasm32"))]
pub mod launcher;

pub use bench::scenario_names as bench_scenario_names;

use crate::game::Game;

/// Routes `--bench`/`--render-note`/`--render-grade` user args into the
/// dev drivers; returns whether one took over the boot.
pub fn dispatch(game: &mut Game, args: &[String]) -> bool {
    let value = |flag: &str| {
        args.iter()
            .position(|arg| arg == flag)
            .and_then(|index| args.get(index + 1))
            .cloned()
    };
    if let Some(path) = value("--profile") {
        profiling::enable(path.into());
    }
    if let Some(scenario) = value("--bench") {
        bench::start(game, &scenario);
        return true;
    }
    if let Some(filter) = value("--render-note") {
        render_note::start(
            game,
            render_note::RenderNoteArgs {
                filter,
                skin: value("--skin"),
                perspective: value("--perspective").unwrap_or_else(|| "None".to_string()),
                bpm: value("--bpm")
                    .and_then(|bpm| bpm.parse().ok())
                    .unwrap_or(120.0),
                out: value("--out").unwrap_or_else(|| "out".to_string()).into(),
                fps: value("--fps")
                    .and_then(|fps| fps.parse().ok())
                    .unwrap_or(60),
            },
        );
        return true;
    }
    if args.iter().any(|arg| arg == "--render-grade") {
        render_grade::start(game);
        return true;
    }
    false
}
