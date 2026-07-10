//! Shared plumbing for the dev launcher binaries: they rebuild the
//! extension, locate a Godot 4 binary, and boot the real game with dev
//! user args — so `cargo run --bin <tool>` always measures and renders the
//! code as it currently stands, never a stale library.

use std::path::PathBuf;
use std::process::Command;

/// The repository root, from the crate the binaries are built in.
pub fn repo_root() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR")
        .expect("dev tools are run via `cargo run --bin <tool>`");
    PathBuf::from(manifest)
        .parent()
        .expect("the crate lives inside the repository")
        .to_path_buf()
}

/// The Godot 4 editor binary the tools boot the game with: `GODOT_BIN`
/// when set, otherwise `godot` on PATH.
pub fn godot_binary() -> String {
    std::env::var("GODOT_BIN").unwrap_or_else(|_| "godot".to_string())
}

/// Rebuilds the extension library so the booted game is never stale.
pub fn build_extension(profile_feature: bool) {
    let root = repo_root();
    let mut command = Command::new("cargo");
    command
        .current_dir(&root)
        .args(["build", "-p", "rhythm", "--lib"]);
    if profile_feature {
        command.args(["--features", "profile"]);
    }
    let status = command.status().expect("failed to run cargo");
    assert!(status.success(), "building the extension failed");
}

/// Boots the game with the given dev user args and waits for it. The
/// child's user data is sandboxed under `sandbox` when given, so tool runs
/// always start from default settings and never touch the player's files.
pub fn run_game(user_args: &[String], sandbox: Option<&PathBuf>) -> std::process::ExitStatus {
    let root = repo_root();
    let mut command = Command::new(godot_binary());
    command
        .current_dir(&root)
        .arg("--path")
        .arg(root.join("godot"))
        .arg("--")
        .args(user_args);
    if let Some(sandbox) = sandbox {
        let _ = std::fs::remove_dir_all(sandbox);
        std::fs::create_dir_all(sandbox).expect("failed to create the sandbox directory");
        command.env("XDG_DATA_HOME", sandbox);
    }
    command
        .status()
        .expect("failed to run godot: install Godot 4 or set GODOT_BIN")
}
