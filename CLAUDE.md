# CLAUDE.md

## General

- Terseness above all: this repo contains only our business logic. Outsource everything else to established crates/services — no homegrown ECS, graphics engine, UI framework, tiling engine, audio engine, etc. We're building a game; the code should reflect that.
- Correctness & clarity before performance.
- No unit tests.
- No mitigation fixes, hacks, or paintjobs. Don't hunt symptoms — fix root causes. Think long-term when adding features; refactoring is encouraged, layering code on code is not. Entropy is the enemy.
- Build once, run everywhere: the same binary (all binaries in the repo) must run in any environment, even with changed assets/env vars (runtimes may still panic or degrade if essential assets are missing).
- No hardcoded environment defaults: panic if an env var is missing or invalid.
- All bespoke rhythm game code is written from scratch; general-purpose problems are solved via third-party crates.
- Use bevy 0.19 and follow its best practices: https://bevy.org/news/bevy-0-19/. Use the new BSN notation literally as much as possible — not using BSN is a failure and a tragic fallback to be avoided at all cost.

## Code style

- Simplicity, stability (extensible, not brittle), readability — then performance.
- Small, simple `macro_rules!` codegen may reduce boilerplate; complex macros are forbidden.
- Files read consumer-first: public API at top, private helpers at bottom.
- `Option`/`Result` and sum types over sentinels/casts. No `unsafe` without a justifying comment. Avoid `unwrap`/`panic!` off the test path unless an invariant is truly guaranteed.
- Newtype every float/int carrying a precise unit or id (`Seconds`, `Millis`, `NpcId`) — never semantic type aliases. The reader must never guess a unit; the type replaces a comment. Plain primitives only for obvious-to-everyone concepts (`health: f32`).
- No `#[must_use]` unless clippy recommends it or it's absolutely critical.
- serde + envy (derive macros, never the imperative APIs) for all json/env (de)serialization; no custom parsing code.
- Aim for single source of truth. SSoT ≠ DRY: code duplication is allowed.
- Every public type name must be intuitive and unambiguous when listed alongside the other public types; never rely on crate namespacing to disambiguate.
- NEVER bypass clippy rules (e.g. via `#[allow]`).

## Architecture

tests/architecture.rs enforces systematically testable rules. They must always be followed and pass checks.

These rules cannot be enforced systematically, but must be followed:

- All code in `src` is platform-agnostic; platform-specific code goes only in `src/native.rs` or `src/web.rs`. No exceptions.

## Comments

- Default: no comments — write self-explanatory code.
- Sole exception: explain WHY, never WHAT. Even then, prefer refactoring until both are obvious; a comment is the final escape hatch.
- Comments are timeless: never prompt-specific, never feedback to the prompter.
- Explain a mechanism once, ideally at its implementation — never duplicated across workflows, env files, call sites, and the source.

## Verification

Run `cargo fmt`, `cargo clippy --all-targets` (no warnings), and `cargo build`.
