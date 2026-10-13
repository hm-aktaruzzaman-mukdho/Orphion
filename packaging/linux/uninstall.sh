#!/usr/bin/env bash
# Removes everything install.sh installed.
set -euo pipefail
PREFIX="${PREFIX:-$HOME/.local}"
APPS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
AUTOSTART_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/autostart"

pkill -x orphion-tray 2>/dev/null || true
rm -fv "$PREFIX/bin/orphion" "$PREFIX/bin/orphion-tray" "$PREFIX/bin/orphionctl"
rm -fv "$APPS/io.github.orphion.Orphion.desktop" "$APPS/io.github.orphion.Orphion.Tray.desktop"
rm -fv "$AUTOSTART_DIR/io.github.orphion.Orphion.Tray.desktop"
rm -fv "${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/256x256/apps/orphion.png"
rm -rfv "${XDG_CONFIG_HOME:-$HOME/.config}/orphion"
