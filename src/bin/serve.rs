//! Development web server for the browser build. Builds the exact site
//! that gets deployed — wasm, JS glue, HTML shell, asset index — and
//! serves it together with the local `assets/` folder. `--emit` writes
//! the whole site out as static files instead, which is how the deploy
//! workflow produces the published site.

use clap::Parser;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

#[derive(Parser)]
struct Cli {
    /// Write the complete static site (game, shell, assets, index) to
    /// this directory and exit, instead of serving. Copies the entire
    /// assets folder.
    #[arg(long)]
    emit: Option<PathBuf>,
    /// Port to serve on.
    #[arg(long, default_value_t = 8080)]
    port: u16,
}

fn main() {
    let cli = Cli::parse();
    let repo = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR")
            .expect("serve is a dev tool: run it via `cargo run --bin serve`"),
    );
    let site = build_site(&repo);
    match cli.emit {
        Some(directory) => emit(&repo, &site, &directory),
        None => serve(&repo, &site, cli.port),
    }
}

/// Compiles the game for the browser and assembles everything except the
/// assets under `target/site/`.
fn build_site(repo: &Path) -> PathBuf {
    run(
        repo,
        "cargo",
        &[
            "build",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
            "--bin",
            "rhythm",
        ],
    );
    let site = repo.join("target/site");
    std::fs::create_dir_all(&site).expect("failed to create target/site");
    run(
        repo,
        "wasm-bindgen",
        &[
            "--target",
            "web",
            "--no-typescript",
            "--out-dir",
            site.to_str().expect("site path is valid unicode"),
            "--out-name",
            "rhythm",
            "target/wasm32-unknown-unknown/release/rhythm.wasm",
        ],
    );
    let wasm = site.join("rhythm_bg.wasm");
    let wasm = wasm.to_str().expect("wasm path is valid unicode");
    run(
        repo,
        "wasm-opt",
        &[
            "-Os",
            "--enable-bulk-memory",
            "--enable-nontrapping-float-to-int",
            "--enable-sign-ext",
            "--enable-mutable-globals",
            wasm,
            "-o",
            wasm,
        ],
    );
    std::fs::copy(repo.join("web/index.html"), site.join("index.html"))
        .expect("failed to copy web/index.html");
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

fn serve(repo: &Path, site: &Path, port: u16) {
    let assets = Arc::new(repo.join("assets"));
    let site = Arc::new(site.to_path_buf());
    let server = tiny_http::Server::http(("127.0.0.1", port))
        .unwrap_or_else(|error| panic!("failed to bind port {port}: {error}"));
    println!("serving the web build at http://127.0.0.1:{port}/");
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

    let response = if path == "assets/index.json" {
        Some((asset_index(assets).into_bytes(), "application/json"))
    } else {
        let file = match path.strip_prefix("assets/") {
            Some(asset) => assets.join(asset),
            None if path.is_empty() => site.join("index.html"),
            None => site.join(&path),
        };
        std::fs::read(&file).ok().map(|bytes| (bytes, mime(&file)))
    };

    let _ = match response {
        Some((bytes, mime)) => request.respond(
            tiny_http::Response::from_data(bytes)
                .with_header(header("Content-Type", mime))
                .with_header(header("Cache-Control", "no-cache")),
        ),
        None => request.respond(tiny_http::Response::empty(404)),
    };
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
        "json" => "application/json",
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
    let mut bytes = Vec::with_capacity(url.len());
    let mut rest = url.bytes();
    while let Some(byte) = rest.next() {
        if byte != b'%' {
            bytes.push(byte);
            continue;
        }
        let high = rest.next().and_then(hex);
        let low = rest.next().and_then(hex);
        match (high, low) {
            (Some(high), Some(low)) => bytes.push(high * 16 + low),
            _ => bytes.push(byte),
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn hex(byte: u8) -> Option<u8> {
    (byte as char).to_digit(16).map(|digit| digit as u8)
}
