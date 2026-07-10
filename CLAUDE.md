# CLAUDE.md

## General

- Terseness above all: this repo contains only our business logic. Outsource everything else to established crates.
- All bespoke rhythm game code is written from scratch; general-purpose problems are solved via third-party crates.
- Correctness & clarity before performance.
- No unit tests.
- No mitigation fixes, hacks, or paintjobs. Don't hunt symptoms — fix root causes. Think long-term when adding features; refactoring is encouraged, layering code on code is not. Entropy is the enemy.
- Build once, run everywhere: the same binary (all binaries in the repo) must run in any environment, even with changed assets/env vars (runtimes may still panic or degrade if essential assets are missing).
- No hardcoded environment defaults: panic if an env var is missing or invalid.
- Use bevy 0.19 and follow its best practices: https://bevy.org/news/bevy-0-19/. Use the new BSN notation literally as much as possible — not using BSN is a failure and a tragic fallback to be avoided at all cost.

## Code style

- Simplicity, stability (extensible, not brittle), readability — then performance.
- Small, simple `macro_rules!` codegen may reduce boilerplate; complex macros are forbidden.
- Files read consumer-first: public API at top, private helpers at bottom.
- `Option`/`Result` and sum types over sentinels/casts.
- No `unsafe` ever.
- Newtype every float/int carrying a precise unit or id (`Seconds`, `Millis`, `NpcId`) — never semantic type aliases. The reader must never guess a unit; the type replaces a comment. Plain primitives only for obvious-to-everyone concepts (`health: f32`).
- serde + envy (derive macros, never the imperative APIs) for all json/env (de)serialization; no custom parsing code.
- Aim for single source of truth. SSoT ≠ DRY: code duplication is allowed.
- Every public type name must be intuitive and unambiguous when listed alongside the other public types; never rely on crate namespacing to disambiguate.
- No `#[must_use]` unless clippy recommends it or it's absolutely critical.
- NEVER bypass clippy rules (e.g. via `#[allow]`).

## Architecture

tests/architecture.rs enforces systematically testable rules. They must always be followed and pass checks.

These rules cannot be enforced systematically, but must be followed:

- All code in `src` is platform-agnostic; platform-specific code goes only in `src/native.rs` or `src/web.rs`. No exceptions.

- `src/prefabs/` holds prefabs: parameterized, reusable visual building blocks. Whenever something is rendered the same way in more than one context, make it a prefab — and design its interface with deliberate intent: options in, ports (driven components/resources) for live inputs, messages out. Prefabs never couple to global state or scenes; everything arrives via `<Name>PrefabOptions` or injected bevy resources. Each prefab colocates all it owns — shaders included, embedded via `embedded_asset!`, never loaded from `assets/` at runtime. Prefabs never depend on each other: compose them from the outside or inject.

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
  with no navigation. They boot the real render code offscreen and dump to
  `out/`, reusing the game's own spawn/shader paths so the output is exactly
  what the game draws. Current ones: `cargo run --bin render_grade`
  (→ `out/grades.png`) and `cargo run --bin render_note <scenario|all>
[--skin .. --bpm ..]` / `--list` (→ `out/*.mp4`). Prefer this: when the thing
  under test can be isolated, add or extend a scenario instead of clicking
  through menus.

- **Live drive harness** — `src/bin/drive.sh` boots the actual windowed game on
  an isolated virtual display and drives it with synthesized input + capture. It
  never touches the real desktop, mutes audio to a null sink, and sandboxes
  settings. Primitives (artifacts land in `out/drive/`):

  ```
  bash src/bin/drive.sh start                 # build + boot, print window id
  bash src/bin/drive.sh key <keysym> [n]      # tap a key n times at the window
  bash src/bin/drive.sh hold <keysym> <secs>  # press-hold-release a key
  bash src/bin/drive.sh shot <name>           # PNG of the window
  bash src/bin/drive.sh rec <name> <secs>     # mp4 of the window
  bash src/bin/drive.sh frames <video> [fps]  # extract stills from an mp4
  bash src/bin/drive.sh strip <out.png> [NxM] <png...>   # tile stills into a sheet
  bash src/bin/drive.sh stop
  ```

The loop: reach the state (`start`, then `key`/`hold`, pausing a beat between
steps) → capture a `shot` for a static look, or `rec` + `frames` for motion and
timing → `strip` many stills into one contact sheet → read that image, compare
to intent, adjust code, repeat. Share the contact sheet as the evidence — both
for interim progress and for the final report.

Gotchas:

- Rebuild before every live run (`start` does this): `clippy`/`fmt` do NOT
  refresh `target/debug/rhythm`, and testing a stale binary is the classic false
  result.
- Only Return-navigable states are reachable on the virtual display — arrow keys
  don't reach the game there; verify arrow-only screens on real hardware.
- Software-Vulkan boot is slow and there's an intro fade; capture only after
  things settle (`start` waits; pause a beat after each state change too).
- For anything animated or timing-sensitive, `rec` then `frames` — a lone
  screenshot can land between frames and mislead.

## Verification

Run `cargo fmt`, `cargo clippy --all-targets` (no warnings), and `cargo build`.
