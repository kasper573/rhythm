//! Renders every grade text to a PNG so the grade shader can be tuned
//! without playing.
//!
//! ```text
//! cargo run --bin render_grade
//! ```

use rhythm::dev::launcher;

fn main() {
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
