#!/usr/bin/env bash
# Removes everything install.sh installed.
set -euo pipefail
PREFIX="${PREFIX:-$HOME/.local}"
APPS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
AUTOSTART_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/autostart"

pkill -x qcy-tray 2>/dev/null || true
rm -fv "$PREFIX/bin/qcy-gui" "$PREFIX/bin/qcy-tray" "$PREFIX/bin/qcyctl"
rm -fv "$APPS/io.github.qcy_linux.Control.desktop" "$APPS/io.github.qcy_linux.Tray.desktop"
rm -fv "$AUTOSTART_DIR/io.github.qcy_linux.Tray.desktop"
rm -fv "${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/256x256/apps/qcy-earbuds.png"
