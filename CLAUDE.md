# CLAUDE.md

## General

- Terseness above all: This repo should contain only our business logic.
  Anything else should be outsourced to well established crates or services. Ie.
  we don't want to build an ECS, a graphics engine, a ui framework, a tiling
  engine, audio engine, etc. We want to build a game, and the code in this repo
  should reflect that.
- Correctness & clarity comes before performance.
- Tests assert on contracts, never implementation details.
- No mitigation fixes or hacks. Refactoring is encouraged: Don't hunt symptoms,
  fix root causes.
- No paintjobs. Think longterm when adding features. Again, refactoring is
  encouraged: Don't just layer code on top of code without thinking about the
  longterm design. Entropy is the enemy.
- Build once, run everywhere. The same binary (applies to all binaries in the
  repo) should be able to run in any environment. If assets or environment
  variables are changed the runtime should work anyway. (Note that runtimes may
  still panic or have degraded behavior if essential assets are missing)
- No hardcoded environment defaults: Panic if an env var is missing or invalid.
  Makes mistakes loud and obvious and forces environments to be well and
  explicitly configured. Also aids with the "build once, run everywhere"
  principle.
- All bespoke rhythm game code must be written from scratch. But general purpose
  problems should be solved via third party crates.
- Use bevy 0.19 for game development. Follow all bevy 0.19 best practices:
  https://bevy.org/news/bevy-0-19/.

## Code style

- Prioritize simplicity, stability (extensible, not brittle), readability — then
  performance.
- small, simple `macro_rules!` codegen is allowed to reduce boilerplate, but
  complex macros are entirely forbidden.
- Files read consumer-first: public API at top, private helpers at the bottom.
- No inline tests: every test lives in its crate's `tests/` folder, against the
  public API.
- Use `Option`/`Result` and sum types over sentinels/casts. No `unsafe` without
  a justifying comment. Avoid `unwrap`/`panic!` off the test path unless an
  invariant is truly guaranteed.
- Newtype every float/int that carries a precise unit or id (`Seconds`,
  `Millis`, `NpcId`) — never semantic type aliases. The reader must not have to
  guess a unit, and the type replaces a comment. Plain primitives are fine only
  for obvious-to-everyone concepts (e.g. `health: f32`).
- Don't use #[must_use]. Only when clippy recommends it or when it's absolutely
  critical.
- Use serde and envy for all json/env serialization and deserialization. No
  custom parsing code. And use the derive macros, not the imperative APIs.
- Aim for single source of truth (however do not conflate this with DRY. Code
  duplication is allowed and is not the same thing as SSoT).
- Any and all public type names must be intuitive and not ambigious if listed
  alongside other public types. Do not rely on crate namespacing to
  disambiguate.
- You are NEVER allowed to bypass clippy rules (ie. via #[allow])

## Architecture

- `src/core` must be self contained and may not depend on anything outside of
  `src/core` (except 3rd party dependencies)
- `src/scenes` may depend on other modules in `src/scenes`, but ideally it's
  kept to a minimum

This is hard absolute rule that you may never break. No exceptions, ever.

Don't work around this constraint by simply putting nothing into `src/core`.
Core is supposed to house generic mechanisms and systems, and you should design
them as configurable/injectable systems so that tiny pieces of integration code
can be wired together in `src/integrations/`

## Comments

- The default mindset should be: Do not write comments. Write code that is self
  explanatory.
- The only exception is: You need to explain WHY, not WHAT some code does.
  However, even then, you should consider refactoring the code so both the WHAT
  and the WHY becomes obvious. Only use comments as a final excape hatch.
- Never use comments as a way to give feedback to the prompter. This means
  comments should never refer to prompt specific details. Comments should be
  timeless and not rely on the reader being the person who prompted you to do
  some work.
- Don't scatter duplicate comments describing how a specific mechanism works all
  over the codebase. Keep it in one place, ideally at the implementation of that
  mechanism. A common source of this type of bad hygiene is re-explaining a
  mechanism in the workflow, in env files, in call sites, and finally also in
  the source code implementation of the mechanism.

## Verification

- `cargo fmt`
- `cargo clippy --all-targets` (no warnings)
- `cargo build`
