//! Frame-rate benchmarks: boots the real game, drives it into a preset
//! scenario, measures five seconds of frame times, and reports fps
//! percentiles. `--profile` additionally captures a chrome trace
//! (building the extension with `--features profile`).

use clap::Parser;
use rhythm::dev::launcher;

#[derive(Parser)]
struct Cli {
    /// The preset scenario to measure; all of them in sequence when
    /// omitted.
    scenario: Option<String>,
    /// Also capture a chrome trace into the working directory.
    #[arg(long)]
    profile: bool,
}

fn main() {
    let cli = Cli::parse();
    let names = rhythm::dev::bench_scenario_names();
    let scenario = cli.scenario.unwrap_or_else(|| "all".to_string());
    assert!(
        scenario == "all" || names.contains(&scenario.as_str()),
        "unknown scenario {scenario:?}; one of: all, {}",
        names.join(", ")
    );

    let mut args = vec!["--bench".to_string(), scenario.clone()];
    if cli.profile {
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

    launcher::build_extension(cli.profile);
    let sandbox = std::env::temp_dir().join("rhythm-bench");
    let status = launcher::run_game(&args, Some(&sandbox));
    std::process::exit(status.code().unwrap_or(1));
}
