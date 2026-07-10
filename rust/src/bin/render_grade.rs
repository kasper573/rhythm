//! Renders every grade text to a PNG so the grade shader can be tuned
//! without playing.
//!
//! ```text
//! cargo run --bin render_grade
//! ```

use rhythm::dev::launcher;

fn main() {
    launcher::build_extension(false);
    let sandbox = std::env::temp_dir().join("rhythm-render");
    let status = launcher::run_game(&["--render-grade".to_string()], Some(&sandbox));
    std::process::exit(status.code().unwrap_or(1));
}
