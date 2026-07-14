# Rhythm

A rhythm game built in Godot 4.7 with .NET (C#), editor-first: a designer edits
scenes, layout, and data in the Godot editor; only shaders, scripts, and the
core mechanics live in code. Conventions and rules: [CLAUDE.md](CLAUDE.md).

## Setup

- .NET 8 SDK, and a Godot 4.7 **.NET** binary as `godot` on `PATH` (or `GODOT_BIN=…`).
- Install the runtime media decoder: `bash tools/fetch-ffmpeg.sh`. Stepfiles ship a
  wide range of video/audio formats, decoded at runtime by the EIRTeam.FFmpeg
  GDExtension; its prebuilt binaries are **fetched** (a pinned release into
  `addons/ffmpeg/`), never vendored in git.
- `ffmpeg` on `PATH` — a separate, **dev-only** dependency, for the render tools below.
- The stepfile library is a drop-in, not in this repo: place it under `assets/stepfiles/`.

## Run

Open the project in the Godot editor, or from the command line:

    dotnet build
    godot --path .

Deep-link any scene, with params, via the launch directives (see `Launch.cs`):

    godot --path . -- --scene stepfile-select
    godot --path . -- --scene play --stepfile "Dance Dance Revolution/Butterfly"

## Layout

The repository root **is** the Godot project (`project.godot`, `Rhythm.csproj`,
`Rhythm.sln`). The split keeps the scenes a designer's and the mechanics a
programmer's:

- `scenes/` — the screens `Game` swaps between, authored in the editor; each
  script holds behavior only. The menus, `StepfileSelect` (the song wheel),
  `Play`, `Score`, `Keymap`, and the `Review` preview scenes.
- `nodes/` — reusable visual building blocks as `[GlobalClass]` nodes (the note
  field, health vial, grade text), each colocating its own `.gdshader`.
- `core/` — the engine-agnostic vocabulary and mechanics as plain C# classes:
  units (`Beat`, `Seconds`, `Bpm`), timing, the stepfile model, scoring.
- `config/` — designer-tunable data as custom `Resource` classes paired with
  their `.tres` values (`GameConfig.tres`).
- `autoload/` — Godot autoloads: settings, high scores, the stepfile library, audio.
- `Launch.cs` — generic launch directives (`--scene` deep links with params,
  input automation, frame reports); the game knows nothing of the tooling on top.
- `tools/` — the dev command line as shell scripts that drive the `godot` binary
  and its movie-maker capture; they never link the game.
- `assets/` — the game's runtime data, loaded from the filesystem and kept out of
  Godot's import/export, so `assets/stepfiles/` stays a drop-in library folder.
- `addons/` — third-party GDExtensions (the EIRTeam.FFmpeg runtime decoder),
  fetched by `tools/fetch-ffmpeg.sh`, not vendored.

## Verify

    dotnet format Rhythm.sln --verify-no-changes
    dotnet build Rhythm.sln                        # zero warnings — warnings are errors

## Tools

The dev command line is shell scripts composing the game's launch directives and
Godot's movie-maker capture (they run the `godot` binary; they never link the game):

    bash tools/dev.sh bench [group/title]                    # fps percentiles for a play session
    bash tools/dev.sh render-note <name|all|--list>          # scenario mp4s   -> out/
    bash tools/dev.sh render-grade                           # the grade sheet -> out/grades.png
    bash tools/dev.sh export <preset> <out>                  # a shippable desktop build
    bash tools/drive.sh start|key|hold|shot|rec|stop         # drive the windowed game on an isolated display

`bench` and the render tools need a display (any X server; the drive harness's
Xvfb works); `export` needs the Godot 4.7 export templates installed.

## Releases

Pushing to `main` builds and publishes desktop archives (Linux and Windows) via
`.github/workflows/deploy.yml`, using Godot's export. Each archive contains the
game and its built-in assets; the player drops their own stepfile library under
`assets/stepfiles/`.
