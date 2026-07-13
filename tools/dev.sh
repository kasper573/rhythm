#!/usr/bin/env bash
# The dev command line: composes the game's launch directives with Godot's
# movie-maker capture and export, by running the `godot` binary. It never
# links the game. A display is needed for the render commands (any X server;
# the drive harness's Xvfb works); `godot` (or $GODOT_BIN) must be a Godot 4.7
# .NET build on PATH, plus ffmpeg for encoding.
#
#   dev.sh render-grade                 # the grade sheet    -> out/grades.png
#   dev.sh render-note <name|all|--list># scenario videos    -> out/<name>.mp4
#   dev.sh export <preset> <out>        # a shippable desktop build
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GODOT="${GODOT_BIN:-godot}"
OUT="$ROOT/out"
MOVIE="$ROOT/out/movie"
FPS=60
WARMUP=20

build() { dotnet build "$ROOT/Rhythm.csproj" -v q -clp:ErrorsOnly >/dev/null; "$GODOT" --headless --path "$ROOT" --import >/dev/null 2>&1 || true; }

# Runs the game headed (movie maker renders), capturing a PNG sequence into a
# fresh movie dir, quitting after the scene ends or `frames` idle frames.
capture() { # capture <frames|""> <scene args...>
  local frames="$1"; shift
  rm -rf "$MOVIE"; mkdir -p "$MOVIE"
  local quit=(); [ -n "$frames" ] && quit=(--quit-after "$frames")
  PULSE_SINK="${DRIVE_SINK:-rhythm_test}" LIBGL_ALWAYS_SOFTWARE=1 \
    "$GODOT" --path "$ROOT" --write-movie "$MOVIE/f.png" --fixed-fps "$FPS" "${quit[@]}" -- "$@" >/dev/null 2>&1 || true
}

# Encodes the captured frames (past the warmup) into an mp4.
encode() { # encode <out.mp4>
  local list; list=$(ls "$MOVIE"/*.png 2>/dev/null | sort | tail -n +$((WARMUP + 1)))
  [ -z "$list" ] && { echo "no frames captured" >&2; return 1; }
  local concat="$MOVIE/frames.txt"; : > "$concat"
  for f in $list; do echo "file '$f'"; echo "duration $(awk "BEGIN{print 1/$FPS}")"; done >> "$concat"
  ffmpeg -y -loglevel error -f concat -safe 0 -i "$concat" -r "$FPS" \
    -c:v libx264 -preset veryfast -crf 18 -pix_fmt yuv420p "$1"
}

render_grade() {
  build; mkdir -p "$OUT"
  capture 120 --scene grade-sheet
  local last; last=$(ls "$MOVIE"/*.png | sort | tail -1)
  cp "$last" "$OUT/grades.png"
  echo "$OUT/grades.png"
}

catalog() { "$GODOT" --headless --path "$ROOT" -- --scene note-demo 2>/dev/null | sed -n 's/^scenario: //p'; }

render_note() {
  build; mkdir -p "$OUT"
  case "${1:-all}" in
    --list) catalog ;;
    all) for name in $(catalog); do render_one "$name"; done ;;
    *) render_one "$1" ;;
  esac
}

render_one() { # render_one <scenario>
  echo "rendering $1" >&2
  capture "" --scene note-demo --scenario "$1"
  encode "$OUT/$1.mp4" && echo "$OUT/$1.mp4"
}

export_build() { # export_build <preset> <out>
  build
  "$GODOT" --headless --path "$ROOT" --export-release "$1" "$(cd "$(dirname "$2")" && pwd)/$(basename "$2")"
}

cmd="${1:-}"; shift || true
case "$cmd" in
  render-grade) render_grade ;;
  render-note) render_note "$@" ;;
  export) export_build "$@" ;;
  *) echo "usage: dev.sh render-grade | render-note <name|all|--list> | export <preset> <out>" >&2; exit 1 ;;
esac
