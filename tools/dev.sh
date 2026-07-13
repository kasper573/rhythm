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

bench() { # bench [--stepfile <group>/<title>]
  build; mkdir -p "$OUT"
  local stepfile="${1:-Dance Dance Revolution/Butterfly}"
  local report="$OUT/frame_report_$$.json"
  PULSE_SINK="${DRIVE_SINK:-rhythm_test}" LIBGL_ALWAYS_SOFTWARE=1 \
    "$GODOT" --headless --path "$ROOT" -- --scene play --stepfile "$stepfile" --pulse P1Up:0.05 --frame-report "$report" --quit-after-seconds 10 >/dev/null 2>&1 || true

  if [ -f "$report" ]; then
    echo "Frame timings:"
    python3 - "$report" <<'PYSCRIPT'
import sys, json, statistics
try:
  with open(sys.argv[1]) as f:
    data = json.load(f)
  frames = sorted(data.get("frames", []))
  if not frames:
    print("  No frames")
    sys.exit(1)
  n = len(frames)
  avg = statistics.mean(frames)
  p50 = frames[int(n * 0.50)]
  p95 = frames[int(n * 0.95)]
  p99 = frames[int(n * 0.99)]
  print(f"  frames: {n}")
  print(f"  avg ms: {avg*1000:.2f} ({1/avg:.1f} fps)")
  print(f"  p50 ms: {p50*1000:.2f}")
  print(f"  p95 ms: {p95*1000:.2f}")
  print(f"  p99 ms: {p99*1000:.2f} ({1/p99:.1f} fps)")
except Exception as e:
  print(f"Error: {e}", file=sys.stderr)
  sys.exit(1)
PYSCRIPT
    rm "$report"
  else
    echo "Frame report not written; check for errors." >&2
    return 1
  fi
}

cmd="${1:-}"; shift || true
case "$cmd" in
  render-grade) render_grade ;;
  render-note) render_note "$@" ;;
  export) export_build "$@" ;;
  bench) bench "$@" ;;
  *) echo "usage: dev.sh render-grade | render-note <name|all|--list> | export <preset> <out> | bench [--stepfile <group>/<title>]" >&2; exit 1 ;;
esac
