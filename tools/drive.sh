#!/usr/bin/env bash
# Live-drive harness for visual testing: boots the REAL windowed game on an
# isolated virtual display and drives it with synthesized input + screen
# capture, so a change can be reviewed exactly as the game renders it. This
# is a dev tool, not shipped code, and is deliberately specific to this Linux
# dev box (Xvfb + llvmpipe + PulseAudio); override the DRIVE_* vars for another.
#
# Safety invariants (each learned the hard way — do not weaken):
#  * DISPLAY is pinned to the virtual display on the first executable line, so
#    no stray xdotool can reach the real desktop. Never inline xdotool in a
#    bash chain: `A && export DISPLAY=:99 & C` backgrounds the whole prefix
#    and runs C on the real display — that once leaked keys into a live app.
#  * Every input targets the game window by id (`--window`); a call made
#    against the wrong display errors out instead of typing into a live app.
#  * Game audio is routed into a null sink, never the speakers.
#  * User data is sandboxed under $WORK/home, so the real user:// directory
#    is untouched and every run starts from default settings.
#
# Commands (see the Visual Testing section of CLAUDE.md for the loop):
#   drive.sh start [-- <game args>]  # build + boot; args deep-link, e.g. -- --scene keymap
#   drive.sh key <keysym> [n]      # tap a key n times at the window
#   drive.sh hold <keysym> <secs>  # press-hold-release a key
#   drive.sh shot <name>           # PNG of the window   -> out/drive/<name>.png
#   drive.sh rec <name> <secs>     # mp4 of the window    -> out/drive/<name>.mp4
#   drive.sh frames <video> [fps]  # extract stills       -> out/drive/<stem>-frames/
#   drive.sh strip <out.png> [NxM] <png...>   # tile stills into one contact sheet
#   drive.sh stop                  # kill the game (leaves Xvfb up for reuse)
#   drive.sh teardown              # kill the game and the virtual display
export DISPLAY="${DRIVE_DISPLAY:-:99}"
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="${DRIVE_OUT:-$ROOT/out/drive}"
SCREEN="${DRIVE_SCREEN:-1280x720x24}"
SINK="${DRIVE_SINK:-rhythm_test}"
SETTLE="${DRIVE_SETTLE:-12}"
GODOT="${GODOT_BIN:-godot}"
mkdir -p "$WORK/home"

wid() { xdotool search --name '^Rhythm' 2>/dev/null | head -1; }

focus() { local w; w="$(wid)"; [ -n "$w" ] && xdotool windowfocus --sync "$w"; echo "$w"; }

start() {
  dotnet build "$ROOT/Rhythm.csproj" -v q -clp:ErrorsOnly || return 1
  "$GODOT" --headless --path "$ROOT" --import >/dev/null 2>&1
  pkill -9 -f "$GODOT --path $ROOT" 2>/dev/null; sleep 0.5
  pgrep -f "Xvfb $DISPLAY" >/dev/null || { Xvfb "$DISPLAY" -screen 0 "$SCREEN" >/dev/null 2>&1 & sleep 1; }
  pactl list short sinks 2>/dev/null | grep -q "$SINK" \
    || pactl load-module module-null-sink sink_name="$SINK" >/dev/null 2>&1
  PULSE_SINK="$SINK" XDG_DATA_HOME="$WORK/home" LIBGL_ALWAYS_SOFTWARE=1 \
    "$GODOT" --path "$ROOT" "$@" >"$WORK/game.log" 2>&1 &
  local w=""
  for _ in $(seq 40); do w="$(wid)"; [ -n "$w" ] && break; sleep 1; done
  [ -z "$w" ] && { echo "no window; tail of game.log:"; tail -15 "$WORK/game.log"; return 1; }
  xdotool windowfocus --sync "$w"; sleep "$SETTLE"
  echo "window=$w  (boot fade settled; log at $WORK/game.log)"
}

key() { # key <keysym> [count] [gap_s]
  local w; w="$(focus)"; [ -z "$w" ] && { echo "no window"; return 1; }
  for _ in $(seq "${2:-1}"); do xdotool key --window "$w" --clearmodifiers "$1"; sleep "${3:-0.18}"; done
}

hold() { # hold <keysym> <secs>
  local w; w="$(focus)"; [ -z "$w" ] && { echo "no window"; return 1; }
  xdotool keydown --window "$w" "$1"; sleep "$2"; xdotool keyup --window "$w" "$1"
}

shot() { # shot <name>
  import -window "$(wid)" "$WORK/$1.png" && echo "$WORK/$1.png"
}

rec() { # rec <name> <secs> [fps]
  local w; w="$(wid)"; local X Y WIDTH HEIGHT
  eval "$(xdotool getwindowgeometry --shell "$w")"
  ffmpeg -y -loglevel error -f x11grab -draw_mouse 0 -framerate "${3:-30}" \
    -video_size "${WIDTH}x${HEIGHT}" -i "$DISPLAY.0+${X},${Y}" -t "$2" \
    -c:v libx264 -preset veryfast -crf 20 -pix_fmt yuv420p "$WORK/$1.mp4" \
    && echo "$WORK/$1.mp4"
}

frames() { # frames <video> [fps]
  local d="$WORK/${1%.*}-frames"; mkdir -p "$d"; rm -f "$d"/*.png
  ffmpeg -y -loglevel error -i "$WORK/$1" -vf "fps=${2:-10}" "$d/%04d.png" && echo "$d"
}

strip() { # strip <out.png> [NxM] <png...>
  local out="$WORK/$1"; shift
  local tile=(); [[ "${1:-}" =~ ^[0-9]+x[0-9]+$ ]] && { tile=(-tile "$1"); shift; }
  montage "$@" "${tile[@]}" -geometry +2+2 -resize "${DRIVE_STRIP_W:-1400}x" "$out" && echo "$out"
}

stop() { pkill -9 -f "$GODOT --path $ROOT" 2>/dev/null; echo stopped; }

teardown() { stop; pkill -f "Xvfb $DISPLAY" 2>/dev/null; echo "torn down $DISPLAY"; }

"$@"
