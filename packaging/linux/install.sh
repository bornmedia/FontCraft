#!/bin/sh
# Installs FontCraft for the current user (no admin rights needed).
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
BIN="$HOME/.local/bin"
APPS="$HOME/.local/share/applications"
mkdir -p "$BIN" "$APPS"
cp "$HERE/fontcraft" "$BIN/fontcraft"
chmod +x "$BIN/fontcraft"
ICONS="$HOME/.local/share/icons/hicolor/512x512/apps"
mkdir -p "$ICONS"
[ -f "$HERE/fontcraft.png" ] && cp "$HERE/fontcraft.png" "$ICONS/fontcraft.png"
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
sed "s|^Exec=.*|Exec=$BIN/fontcraft|" "$HERE/fontcraft.desktop" > "$APPS/fontcraft.desktop"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APPS" || true
echo "FontCraft is installed. Find it in your app menu, or run: $BIN/fontcraft"
