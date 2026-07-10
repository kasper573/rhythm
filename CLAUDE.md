# CLAUDE.md

## General

- Terseness above all: this repo contains only our business logic. Outsource everything else to established crates.
- All bespoke rhythm game code is written from scratch; general-purpose problems are solved via third-party crates.
- Correctness & clarity before performance.
- No unit tests.
- No mitigation fixes, hacks, or paintjobs. Don't hunt symptoms — fix root causes. Think long-term when adding features; refactoring is encouraged, layering code on code is not. Entropy is the enemy.
- Build once, run everywhere: the same binary (all binaries in the repo) must run in any environment, even with changed assets/env vars (runtimes may still panic or degrade if essential assets are missing).
- No hardcoded environment defaults: panic if an env var is missing or invalid.
- The game is a Godot 4.7 GDExtension written with godot-rust (the `godot` crate) and follows Godot's best practices: custom node classes, signals, Control-based layout, `user://` persistence, InputMap actions, audio buses. Godot-isms over home-grown machinery is the rule — reaching around the engine is a failure.

## Layout

- `rust/` — the extension crate (`cdylib` + the dev launcher binaries), following the godot-rust convention of a Rust crate beside the Godot project.
- `godot/` — the Godot project: `project.godot`, the boot scene, `rhythm.gdextension`, and `export_presets.cfg`. All game logic stays in Rust; the project holds configuration only.
- `assets/` — the game's data, loaded at runtime from the filesystem (or over HTTP on the web); deliberately not packed into the export so a shipped build's `assets/stepfiles/` stays a drop-in library folder.

## Code style

- Simplicity, stability (extensible, not brittle), readability — then performance.
- Small, simple `macro_rules!` codegen may reduce boilerplate; complex macros are forbidden.
- Files read consumer-first: public API at top, private helpers at bottom.
- `Option`/`Result` and sum types over sentinels/casts.
- Newtype every float/int carrying a precise unit or id (`Seconds`, `Millis`, `NpcId`) — never semantic type aliases. The reader must never guess a unit; the type replaces a comment. Plain primitives only for obvious-to-everyone concepts (`health: f32`).
- serde + envy (derive macros, never the imperative APIs) for all json/env (de)serialization; no custom parsing code.
- Aim for single source of truth. SSoT ≠ DRY: code duplication is allowed.
- Every public type name must be intuitive and unambiguous when listed alongside the other public types; never rely on crate namespacing to disambiguate.
- No `#[must_use]` unless clippy recommends it or it's absolutely critical.
- NEVER bypass clippy rules (e.g. via `#[allow]`).

## Architecture

rust/tests/architecture.rs enforces systematically testable rules. They must always be followed and pass checks.

These rules cannot be enforced systematically, but must be followed:

- All code in `rust/src` is platform-agnostic; platform-specific code goes only in `rust/src/native.rs` or `rust/src/web.rs`. No exceptions.

- `rust/src/nodes/` holds the game's custom nodes: parameterized, reusable visual building blocks. Whenever something is rendered the same way in more than one context, make it a node — and design its interface with deliberate intent: options in (`<Name>Options` into `instantiate`), ports (methods the owner drives every frame) for live inputs, signals out. Nodes never couple to global scene state; everything arrives via options, ports, or the core singletons. Each node colocates all it owns — shaders included, embedded via `include_str!`, never loaded from `assets/` at runtime. Nodes never depend on each other: compose them from the outside or inject.

- Scenes (`rust/src/scenes/`) are swapped by the `Game` root and discard all state on teardown; anything a scene hands the next one travels as a consumed route param through `Game`.

## Comments

- Default: no comments — write self-explanatory code.
- Sole exception: explain WHY, never WHAT. Even then, prefer refactoring until both are obvious; a comment is the final escape hatch.
- Comments are timeless: never prompt-specific, never feedback to the prompter.
- Explain a mechanism once, ideally at its implementation — never duplicated across workflows, env files, call sites, and the source.

## Visual Testing

Prove a visual or behavioural change by _looking at what the game renders_, not
by reasoning alone. Every path below ends in a PNG you read back with the image
tool and compare against the intent.

Two capture paths:

- **Headless render binaries** — for one component or animation in isolation,
  with no navigation. They rebuild the extension, boot the real game offscreen
  in a dev mode, and dump to `out/`, reusing the game's own node/shader paths
  so the output is exactly what the game draws. Current ones: `cargo run --bin
render_grade` (→ `out/grades.png`) and `cargo run --bin render_note
<scenario|all> [--skin .. --bpm ..]` / `--list` (→ `out/*.mp4`). Prefer this:
  when the thing under test can be isolated, add or extend a scenario instead
  of clicking through menus. They need a display (any X server; the drive
  harness's Xvfb works) and a Godot 4 binary (`godot` on PATH or `GODOT_BIN`).

- **Live drive harness** — `rust/src/bin/drive.sh` boots the actual windowed
  game on an isolated virtual display and drives it with synthesized input +
  capture. It never touches the real desktop, mutes audio to a null sink, and
  sandboxes user data. Primitives (artifacts land in `out/drive/`):

  ```
  bash rust/src/bin/drive.sh start                 # build + boot, print window id
  bash rust/src/bin/drive.sh key <keysym> [n]      # tap a key n times at the window
  bash rust/src/bin/drive.sh hold <keysym> <secs>  # press-hold-release a key
  bash rust/src/bin/drive.sh shot <name>           # PNG of the window
  bash rust/src/bin/drive.sh rec <name> <secs>     # mp4 of the window
  bash rust/src/bin/drive.sh frames <video> [fps]  # extract stills from an mp4
  bash rust/src/bin/drive.sh strip <out.png> [NxM] <png...>   # tile stills into a sheet
  bash rust/src/bin/drive.sh stop
  ```

The loop: reach the state (`start`, then `key`/`hold`, pausing a beat between
steps) → capture a `shot` for a static look, or `rec` + `frames` for motion and
timing → `strip` many stills into one contact sheet → read that image, compare
to intent, adjust code, repeat. Share the contact sheet as the evidence — both
for interim progress and for the final report.

Gotchas:

- Rebuild before every live run (`start` does this): `clippy`/`fmt` do NOT
  refresh the extension library, and testing a stale build is the classic false
  result.
- Xvfb's keymap has no arrow keys, so arrow-bound states are unreachable there
  by default; seed the sandbox's `machine_settings.json` with letter-key
  overrides for P1 (the keymap holds overrides over the config defaults) to
  reach everything, or verify arrow-only flows on real hardware.
- Software-GL boot is slow and there's an intro fade; capture only after
  things settle (`start` waits; pause a beat after each state change too).
- For anything animated or timing-sensitive, `rec` then `frames` — a lone
  screenshot can land between frames and mislead.

## Verification

Run `cargo fmt`, `cargo clippy --all-targets` (no warnings), and `cargo build`.
Boot checks run the game from the project: `godot --path godot`.
