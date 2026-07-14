#!/usr/bin/env bash
# Installs the EIRTeam.FFmpeg GDExtension — the runtime decoder that lets the
# game play the full range of video/audio/image formats stepfiles ship (avi,
# mpg, mp4, ...). The prebuilt binaries are NOT vendored in git; this fetches a
# pinned release into addons/ffmpeg/. Run once after cloning (and in CI before
# building or booting the game). The ffmpeg *CLI* the dev capture tools use is a
# separate, dev-only dependency.
set -euo pipefail

TAG="autobuild-2025-11-12-13-44"        # EIRTeam.FFmpeg 1.1.4
ASSET="eirteam-ffmpeg-1.1.4.zip"
SHA256="1a8dbc4d7524172ca72517dac4ffb24965025c2f19067882be35376b75bc107c"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT/addons/ffmpeg"
MARKER="$DEST/.installed-$TAG"
URL="https://github.com/EIRTeam/EIRTeam.FFmpeg/releases/download/$TAG/$ASSET"

if [ -f "$MARKER" ]; then
    echo "EIRTeam.FFmpeg $TAG already installed."
    exit 0
fi

echo "Fetching EIRTeam.FFmpeg $TAG..."
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
curl -fsSL "$URL" -o "$tmp/ffmpeg.zip"
echo "$SHA256  $tmp/ffmpeg.zip" | sha256sum -c - >/dev/null
rm -rf "$DEST"
unzip -q "$tmp/ffmpeg.zip" -d "$ROOT"
touch "$MARKER"
echo "Installed EIRTeam.FFmpeg into addons/ffmpeg/"
