#!/bin/sh
# Installs FontCraft for the current user (no admin rights needed).
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
BIN="$HOME/.local/bin"
APPS="$HOME/.local/share/applications"
mkdir -p "$BIN" "$APPS"
cp "$HERE/fontcraft" "$BIN/fontcraft"
chmod +x "$BIN/fontcraft"
sed "s|^Exec=.*|Exec=$BIN/fontcraft|" "$HERE/fontcraft.desktop" > "$APPS/fontcraft.desktop"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APPS" || true
echo "FontCraft is installed. Find it in your app menu, or run: $BIN/fontcraft"
