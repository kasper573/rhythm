//! Development web server for the browser build. Builds the exact site
//! that gets deployed — the Godot web export plus the asset tree and its
//! index — and serves it together with the local `assets/` folder.
//! `--emit` writes the whole site out as static files instead, which is
//! how the deploy workflow produces the published site.
//!
//! Building the wasm extension needs the nightly toolchain with the
//! `wasm32-unknown-emscripten` target, an emsdk matching the Godot
//! version's emscripten, and Godot 4 export templates installed.

use clap::Parser;
use rhythm::dev::launcher;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

#[derive(Parser)]
struct Cli {
    /// Write the complete static site (game, assets, index) to this
    /// directory and exit, instead of serving. Copies the entire assets
    /// folder.
    #[arg(long)]
    emit: Option<PathBuf>,
    /// Interface to bind; 0.0.0.0 exposes the server to the LAN for
    /// testing from other devices.
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    /// Port to serve on.
    #[arg(long, default_value_t = 8080)]
    port: u16,
}

fn main() {
    let cli = Cli::parse();
    let repo = launcher::repo_root();
    let site = build_site(&repo);
    match cli.emit {
        Some(directory) => emit(&repo, &site, &directory),
        None => serve(&repo, &site, &cli.host, cli.port),
    }
}

/// Compiles the extension for the browser and exports the web build to
/// `target/site/`.
fn build_site(repo: &Path) -> PathBuf {
    run(
        repo,
        "cargo",
        &[
            "+nightly",
            "build",
            "-p",
            "rhythm",
            "--lib",
            "-Zbuild-std",
            "--target",
            "wasm32-unknown-emscripten",
            "--release",
        ],
    );
    let site = repo.join("target/site");
    std::fs::create_dir_all(&site).expect("failed to create target/site");
    let godot = launcher::godot_binary();
    let project = repo.join("godot").display().to_string();
    run(
        repo,
        &godot,
        &["--headless", "--path", &project, "--import"],
    );
    run(
        repo,
        &godot,
        &[
            "--headless",
            "--path",
            &project,
            "--export-release",
            "Web",
            &site.join("index.html").display().to_string(),
        ],
    );
    site
}

fn run(cwd: &Path, program: &str, args: &[&str]) {
    println!("$ {program} {}", args.join(" "));
    let status = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .status()
        .unwrap_or_else(|error| panic!("failed to run {program}: {error}"));
    assert!(status.success(), "{program} failed");
}

/// The listing the web platform boots from: every file under the asset
/// root, as forward-slashed relative paths.
fn asset_index(assets: &Path) -> String {
    let mut files: Vec<String> = walkdir::WalkDir::new(assets)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| {
            let relative = entry.path().strip_prefix(assets).ok()?;
            Some(relative.to_string_lossy().replace('\\', "/"))
        })
        .collect();
    files.sort();
    serde_json::json!({ "files": files }).to_string()
}

fn emit(repo: &Path, site: &Path, directory: &Path) {
    let assets = repo.join("assets");
    for entry in walkdir::WalkDir::new(site)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let target = directory.join(
            entry
                .path()
                .strip_prefix(site)
                .expect("site files live under the site directory"),
        );
        copy_into(entry.path(), &target);
    }
    for entry in walkdir::WalkDir::new(&assets)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let target = directory.join("assets").join(
            entry
                .path()
                .strip_prefix(&assets)
                .expect("asset files live under the assets directory"),
        );
        copy_into(entry.path(), &target);
    }
    std::fs::write(directory.join("assets/index.json"), asset_index(&assets))
        .expect("failed to write the asset index");
    println!("site emitted to {}", directory.display());
}

fn copy_into(source: &Path, target: &Path) {
    std::fs::create_dir_all(target.parent().expect("target file has a parent"))
        .expect("failed to create a site directory");
    std::fs::copy(source, target)
        .unwrap_or_else(|error| panic!("failed to copy {}: {error}", source.display()));
}

fn serve(repo: &Path, site: &Path, host: &str, port: u16) {
    let assets = Arc::new(repo.join("assets"));
    let site = Arc::new(site.to_path_buf());
    let server = tiny_http::Server::http((host, port))
        .unwrap_or_else(|error| panic!("failed to bind {host}:{port}: {error}"));
    println!("serving the web build at http://{host}:{port}/");
    for request in server.incoming_requests() {
        let assets = assets.clone();
        let site = site.clone();
        std::thread::spawn(move || respond(request, &site, &assets));
    }
}

fn respond(request: tiny_http::Request, site: &Path, assets: &Path) {
    let url = request.url().split('?').next().unwrap_or("/");
    let path = percent_decode(url.trim_start_matches('/'));
    if path.split('/').any(|component| component == "..") {
        let _ = request.respond(tiny_http::Response::empty(404));
        return;
    }

    let content = if path == "assets/index.json" {
        Some((asset_index(assets).into_bytes(), "application/json"))
    } else {
        let file = match path.strip_prefix("assets/") {
            Some(asset) => assets.join(asset),
            None if path.is_empty() => site.join("index.html"),
            None => site.join(&path),
        };
        std::fs::read(&file).ok().map(|bytes| (bytes, mime(&file)))
    };
    let Some((bytes, mime)) = content else {
        let _ = request.respond(tiny_http::Response::empty(404));
        return;
    };

    // iOS Safari refuses to play media from servers without byte-range
    // support, so single ranges are honored for every file.
    let range = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("range"))
        .and_then(|header| parse_range(header.value.as_str(), bytes.len()));

    let response = match range {
        Some((start, end)) => tiny_http::Response::from_data(bytes[start..=end].to_vec())
            .with_status_code(206)
            .with_header(header(
                "Content-Range",
                &format!("bytes {start}-{end}/{}", bytes.len()),
            )),
        None => tiny_http::Response::from_data(bytes),
    };
    let _ = request.respond(
        response
            .with_header(header("Content-Type", mime))
            .with_header(header("Cache-Control", "no-cache"))
            .with_header(header("Accept-Ranges", "bytes"))
            // Cross-origin isolation: SharedArrayBuffer (and with it any
            // future wasm threading) only exists on pages served with
            // these headers.
            .with_header(header("Cross-Origin-Opener-Policy", "same-origin"))
            .with_header(header("Cross-Origin-Embedder-Policy", "require-corp")),
    );
}

/// The byte window of a `Range: bytes=a-b` header (also `a-` and `-n`
/// forms), inclusive and clamped; `None` falls back to the whole file.
fn parse_range(value: &str, total: usize) -> Option<(usize, usize)> {
    let spec = value.strip_prefix("bytes=")?.split(',').next()?.trim();
    let (start, end) = spec.split_once('-')?;
    let last = total.checked_sub(1)?;
    if start.is_empty() {
        let suffix: usize = end.parse().ok()?;
        return Some((total.saturating_sub(suffix.max(1)), last));
    }
    let start: usize = start.parse().ok()?;
    if start > last {
        return None;
    }
    let end: usize = match end {
        "" => last,
        end => end.parse::<usize>().ok()?.min(last),
    };
    (start <= end).then_some((start, end))
}

fn header(field: &str, value: &str) -> tiny_http::Header {
    tiny_http::Header::from_bytes(field.as_bytes(), value.as_bytes())
        .expect("static header is valid")
}

fn mime(path: &Path) -> &'static str {
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript",
        "wasm" => "application/wasm",
        "pck" => "application/octet-stream",
        "json" => "application/json",
        "glb" => "model/gltf-binary",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "ogg" => "audio/ogg",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "avi" => "video/x-msvideo",
        "mpg" | "mpeg" => "video/mpeg",
        _ => "text/plain; charset=utf-8",
    }
}

/// Browsers percent-encode asset URLs (song folders contain spaces and
/// non-ASCII titles); decode them back into filesystem names.
fn percent_decode(url: &str) -> String {
    percent_encoding::percent_decode_str(url)
        .decode_utf8_lossy()
        .into_owned()
}
