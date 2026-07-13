# Rhythm

A rhythm game: Godot 4.7 GDExtension, all logic in Rust (godot-rust).
Rules and architecture: [CLAUDE.md](CLAUDE.md).

## Setup

- Rust stable; a Godot 4.7 binary as `godot` on PATH (or `GODOT_BIN=…`); `ffmpeg` on PATH.
- Stepfile library (drop-in, not in this repo) into `assets/stepfiles/`.
- Once per clone — Godot only loads extensions it has on record
  ([godotengine/godot#81478](https://github.com/godotengine/godot/issues/81478)):

      mkdir -p godot/.godot && echo 'res://rhythm.gdextension' > godot/.godot/extension_list.cfg

## Run

    cargo build
    godot --path godot

Deep-link any scene with params: `godot --path godot -- --scene wheel` (see `rust/src/launch.rs`).

## Verify

    cargo fmt --all
    cargo clippy --workspace --all-targets    # zero warnings
    cargo test -p rhythm --test architecture

## Tools

    cargo run -p tools -- bench [scenario]                    # fps percentiles, JSON on stdout
    cargo run -p tools -- render-note <filter|all> [--list]   # scenario mp4s -> out/
    cargo run -p tools -- render-grade                        # grade sheet   -> out/grades.png
    cargo run -p tools -- serve [--host 0.0.0.0]              # web build; LAN = HTTPS, accept the cert once
    cargo run -p tools -- export <preset> <out>               # shippable desktop build
    bash tools/drive.sh start|key|hold|shot|rec|stop          # drive the windowed game on an isolated display

Bench/render need a display; `serve`/`export` need Godot 4.7 export templates. Web builds
additionally need the `nightly-2026-01-01` toolchain (`rust-src`, `wasm32-unknown-emscripten`)
and an active emsdk 3.1.74.
