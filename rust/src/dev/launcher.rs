//! Shared plumbing for the dev command line: it rebuilds the extension,
//! locates a Godot 4 binary, and boots the real game with dev user args —
//! so `cargo run -- <tool>` always measures and renders the code as it
//! currently stands, never a stale library.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The repository root, from the crate the binaries are built in.
pub fn repo_root() -> PathBuf {
    let manifest =
        std::env::var("CARGO_MANIFEST_DIR").expect("dev tools are run via `cargo run -- <tool>`");
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

/// Builds the shipped (release) extension library.
pub fn build_extension_release() {
    let status = Command::new("cargo")
        .current_dir(repo_root())
        .args(["build", "-p", "rhythm", "--lib", "--release"])
        .status()
        .expect("failed to run cargo");
    assert!(status.success(), "building the extension failed");
}

/// Imports the project and exports one release preset with headless
/// Godot to `output` (Godot requires its directory to already exist and
/// resolves relative paths against the project, hence absolute).
pub fn export_release(preset: &str, output: &Path) {
    let project = repo_root().join("godot");
    write_extension_manifest(&project);
    let output = std::path::absolute(output).expect("the export path resolves");
    if let Some(directory) = output.parent() {
        std::fs::create_dir_all(directory).expect("failed to create the export directory");
    }
    let godot = godot_binary();
    let project = project.display().to_string();
    let output = output.display().to_string();
    run(&godot, &["--headless", "--path", &project, "--import"]);
    run(
        &godot,
        &[
            "--headless",
            "--path",
            &project,
            "--export-release",
            preset,
            &output,
        ],
    );
}

/// Boots the game with the given dev user args and waits for it. The
/// child's user data is sandboxed under `sandbox` when given, so tool runs
/// always start from default settings and never touch the player's files.
pub fn run_game(user_args: &[String], sandbox: Option<&PathBuf>) -> std::process::ExitStatus {
    let root = repo_root();
    write_extension_manifest(&root.join("godot"));
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

/// Godot loads GDExtensions from this project-data manifest, which
/// normally appears once the editor has scanned the project. A boot that
/// instead discovers the extension mid-scan crashes the editor
/// (godotengine/godot#81478), and game-mode boots skip discovery entirely
/// and run without the extension — so every launcher writes the manifest
/// up front, making even the first boot take the ordinary early-load path.
fn write_extension_manifest(project: &Path) {
    let data = project.join(".godot");
    std::fs::create_dir_all(&data).expect("failed to create the project data directory");
    std::fs::write(
        data.join("extension_list.cfg"),
        "res://rhythm.gdextension\n",
    )
    .expect("failed to write the extension manifest");
}

fn run(program: &str, args: &[&str]) {
    println!("$ {program} {}", args.join(" "));
    let status = Command::new(program)
        .args(args)
        .status()
        .unwrap_or_else(|error| panic!("failed to run {program}: {error}"));
    assert!(status.success(), "{program} failed");
}
