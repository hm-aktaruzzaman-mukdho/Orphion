#!/usr/bin/env bash
# Removes everything install.sh installed.
set -euo pipefail
PREFIX="${PREFIX:-$HOME/.local}"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
APPS="$DATA/applications"
AUTOSTART_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/autostart"

# Stops the window and the tray (both are the orphion program), and the
# separate tray of earlier versions.
pkill -x orphion 2>/dev/null || true
pkill -x orphion-tray 2>/dev/null || true
rm -fv "$PREFIX/bin/orphion" "$PREFIX/bin/orphionctl" "$PREFIX/bin/orphion-tray"
rm -fv "$APPS/io.github.orphion.Orphion.desktop" "$APPS/io.github.orphion.Orphion.Tray.desktop"
rm -fv "$AUTOSTART_DIR/io.github.orphion.Orphion.Tray.desktop"
rm -fv "$DATA/icons/hicolor/256x256/apps/orphion.png"
rm -rfv "${XDG_CONFIG_HOME:-$HOME/.config}/orphion"
