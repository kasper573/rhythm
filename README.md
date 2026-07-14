# Rhythm

A rhythm game built in Godot 4.7 with .NET (C#), editor-first: a designer edits
scenes, layout, and data in the Godot editor; only shaders, scripts, and the
core mechanics live in code. Rules and architecture: [CLAUDE.md](CLAUDE.md).

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
