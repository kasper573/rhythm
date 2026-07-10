//! Exports a shippable release build of one preset from
//! `godot/export_presets.cfg` via headless Godot.
//!
//! ```text
//! cargo run --bin export -- Linux target/export/linux/rhythm.x86_64
//! ```

use clap::Parser;
use rhythm::dev::launcher;
use std::path::PathBuf;

#[derive(Parser)]
struct Cli {
    /// Preset name from godot/export_presets.cfg.
    preset: String,
    /// The exported game binary's path.
    out: PathBuf,
}

fn main() {
    let cli = Cli::parse();
    // The editor performing the export loads the debug library; the
    // exported game ships the release one.
    launcher::build_extension(false);
    launcher::build_extension_release();
    launcher::export_release(&cli.preset, &cli.out);
}
