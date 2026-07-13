# CLAUDE.md

## General

- Terseness above all: this repo contains only our business logic. Outsource everything else to established packages.
- All bespoke rhythm game code is written from scratch; general-purpose problems are solved via third-party packages.
- Correctness & clarity before performance.
- No unit tests.
- No mitigation fixes, hacks, or paintjobs. Don't hunt symptoms — fix root causes. Think long-term when adding features; refactoring is encouraged, layering code on code is not. Entropy is the enemy.
- Build once, run everywhere: the same export (all binaries in the repo) must run in any environment, even with changed assets/env vars (runtimes may still panic or degrade if essential assets are missing).
- No hardcoded environment defaults: panic if an env var is missing or invalid.
- The game is a Godot 4.7 .NET (C#) project and follows Godot's best practices: editor-authored scenes, custom node classes, signals, Control-based layout, `user://` persistence, InputMap actions, audio buses. Godot-isms over home-grown machinery is the rule — reaching around the engine is a failure.

## Editor-first

The game is built for a non-coding game designer to edit in the Godot editor; the tooling is built for programmers. Concretely:

- Scenes are authored in the editor and `.tscn` is the source of truth: layout via containers/anchors/theme, transitions via `AnimationPlayer`, repeated elements as instanced sub-scenes. Scripts hold behavior only — a scene script that constructs layout is a defect.
- Everything visual is designer-editable: every `[Export]` carries hints and groups so inspectors read like settings panels; visual nodes are `[Tool]` classes that render a meaningful editor preview; misconfiguration reports through `_GetConfigurationWarnings()`, not runtime errors.
- Designer-tunable data lives in custom `Resource` classes under `godot/data` (`.tres`), not in code and not in ad-hoc JSON.
- Mechanics stay code: judgment, timing, input, and note-field internals expose tunables as exports but never surrender their invariants to the scene tree.
- Custom asset formats (stepfiles, noteskins) are editor citizens through import plugins (`godot/addons`) that reuse the runtime parsers from `core/`.

## Layout

- `godot/` — the Godot project and all game code: editor-authored scenes, custom nodes, autoloads, designer data, import plugins. The game exposes generic launch directives (`godot/Launch.cs`: `--scene` deep links with params, `--pulse`/`--hold` input automation, `--frame-report`, `--quit-after-seconds`) and knows nothing about the tooling built on them.
- `core/` — the engine-free game vocabulary and mechanics (units, timing, stepfile model, scoring, library) as a plain .NET library; referenced by the game and the tools, referencing nothing engine-shaped.
- `tools/` — the dev command line (`dotnet run --project tools -- bench|render-note|render-grade|export`) plus `drive.sh`; it composes the game's launch directives with Godot's movie-maker capture and never references the game assembly.
- `assets/` — the game's data, loaded at runtime from the filesystem; deliberately not imported into the Godot project nor packed into exports, so a shipped build's `assets/stepfiles/` stays a drop-in library folder.

## Code style

- Simplicity, stability (extensible, not brittle), readability — then performance.
- Files read consumer-first: public API at top, private helpers at bottom.
- Nullable reference types on everywhere; nullability and exhaustive `switch` expressions over sentinels and casts.
- Newtype every float/int carrying a precise unit or id (`Seconds`, `Beat`, `Bpm`) as a `readonly record struct` — never a bare `double`. The reader must never guess a unit; the type replaces a comment. Plain primitives only for obvious-to-everyone concepts (`float health`).
- Attribute-driven (de)serialization (System.Text.Json) for all JSON; env vars are read explicitly and validated at boot; no custom parsing code.
- Aim for single source of truth. SSoT ≠ DRY: code duplication is allowed.
- Every public type name must be intuitive and unambiguous when listed alongside the other public types; never rely on namespacing to disambiguate.
- Zero warnings, enforced (`TreatWarningsAsErrors`); NEVER suppress a diagnostic (`#pragma warning`, `[SuppressMessage]`).

## Architecture

`tests/` enforces systematically testable rules. They must always be followed and pass checks.

These rules cannot be enforced systematically, but must be followed:

- All code in `core/` is engine-agnostic; anything touching a Godot API lives in `godot/`.
- `godot/nodes/` holds the game's custom nodes: parameterized, reusable visual building blocks. Whenever something is rendered the same way in more than one context, make it a node — and design its interface with deliberate intent: options in (`[Export]` properties), ports (methods the owner drives every frame) for live inputs, signals out. Nodes never couple to global scene state; everything arrives via exports, ports, or the autoloads. Each node colocates all it owns — shaders included, as `.gdshader` files beside the script. Nodes never depend on each other: compose them from the outside or inject.
- Scenes (`godot/scenes/`) are swapped by the `Game` root and discard all state on teardown; anything a scene hands the next one travels as a consumed route param through `Game`.

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

- **Render tools** — for one component or animation in isolation, with no
  navigation. They rebuild the game, deep-link it into a review scene
  (`grade-sheet`, `note-demo`), and capture it deterministically with Godot's
  movie maker into `out/`, reusing the game's own node/shader paths so the
  output is exactly what the game draws. `dotnet run --project tools --
  render-grade` (→ `out/grades.png`) and `dotnet run --project tools --
  render-note <scenario|all> [--skin .. --bpm ..]` / `--list` (→ `out/*.mp4`).
  Prefer this: when the thing under test can be isolated, add or extend a
  scenario instead of clicking through menus. They need a display (any X
  server; the drive harness's Xvfb works), the Godot 4.7 .NET binary (`godot`
  on PATH or `GODOT_BIN`), and ffmpeg on the PATH. To reach any other state
  directly, deep-link it: `godot --path godot -- --scene wheel` (see
  `godot/Launch.cs`).

- **Live drive harness** — `tools/drive.sh` boots the actual windowed
  game on an isolated virtual display and drives it with synthesized input +
  capture. It never touches the real desktop, mutes audio to a null sink, and
  sandboxes user data. Primitives (artifacts land in `out/drive/`):

  ```
  bash tools/drive.sh start                 # build + boot, print window id
  bash tools/drive.sh key <keysym> [n]      # tap a key n times at the window
  bash tools/drive.sh hold <keysym> <secs>  # press-hold-release a key
  bash tools/drive.sh shot <name>           # PNG of the window
  bash tools/drive.sh rec <name> <secs>     # mp4 of the window
  bash tools/drive.sh frames <video> [fps]  # extract stills from an mp4
  bash tools/drive.sh strip <out.png> [NxM] <png...>   # tile stills into a sheet
  bash tools/drive.sh stop
  ```

The loop: reach the state (`start`, then `key`/`hold`, pausing a beat between
steps) → capture a `shot` for a static look, or `rec` + `frames` for motion and
timing → `strip` many stills into one contact sheet → read that image, compare
to intent, adjust code, repeat. Share the contact sheet as the evidence — both
for interim progress and for the final report.

Gotchas:

- Rebuild before every live run (`start` does this): `dotnet format` does NOT
  refresh the game assembly, and testing a stale build is the classic false
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

Run `dotnet format Rhythm.sln --verify-no-changes`, `dotnet build Rhythm.sln`
(zero warnings — warnings are errors), and `dotnet test tests` (architecture
rules). Boot checks run the game from the project: `godot --path godot`.
