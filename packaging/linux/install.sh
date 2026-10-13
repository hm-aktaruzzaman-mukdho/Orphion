#!/usr/bin/env bash
# Installs Orphion for the current user (no root needed). From the source
# tree it builds first; from a release tarball (made by build-linux.sh) it
# installs the prebuilt binaries in bin/.
#
#   packaging/linux/install.sh              install orphion and orphionctl
#   packaging/linux/install.sh --autostart  also start the tray icon at login
#                                           (and right away)
#
# PREFIX defaults to ~/.local (binaries in ~/.local/bin).
set -euo pipefail

# Work from the repository (or tarball) root, two levels up.
cd "$(dirname "$0")/../.."
PREFIX="${PREFIX:-$HOME/.local}"
BIN="$PREFIX/bin"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
APPS="$DATA/applications"
AUTOSTART_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/autostart"
TRAY_ENTRY=io.github.orphion.Orphion.Tray.desktop

if [[ -x bin/orphion && -x bin/orphionctl ]]; then
    SRC=bin
else
    echo "==> Building (release)…"
    cargo build --release --workspace
    SRC=target/release
fi

AUTOSTART=false
[[ "${1:-}" == "--autostart" ]] && AUTOSTART=true

# Versions before the rename were called "QCY Earbuds" (qcy-gui, qcy-tray,
# qcyctl). Replace them, keeping the tray's autostart if it was enabled.
LEGACY_AUTOSTART="$AUTOSTART_DIR/io.github.qcy_linux.Tray.desktop"
[[ -e "$LEGACY_AUTOSTART" ]] && AUTOSTART=true
if [[ -e "$BIN/qcy-gui" || -e "$BIN/qcy-tray" || -e "$BIN/qcyctl" ]]; then
    echo "==> Removing the previous version (QCY Earbuds)"
    pkill -x qcy-tray 2>/dev/null || true
    rm -f "$BIN/qcy-gui" "$BIN/qcy-tray" "$BIN/qcyctl" "$LEGACY_AUTOSTART" \
        "$APPS/io.github.qcy_linux.Control.desktop" "$APPS/io.github.qcy_linux.Tray.desktop" \
        "$DATA/icons/hicolor/256x256/apps/qcy-earbuds.png"
fi

# Earlier Orphion versions had a separate orphion-tray program with its own
# menu entry. The tray is now `orphion --tray`; an autostart entry for the
# old program becomes one for the new mode.
if [[ -e "$BIN/orphion-tray" || -e "$APPS/$TRAY_ENTRY" ]]; then
    echo "==> Removing the separate orphion-tray program"
    grep -qs "orphion-tray" "$AUTOSTART_DIR/$TRAY_ENTRY" && AUTOSTART=true
    pkill -x orphion-tray 2>/dev/null || true
    rm -f "$BIN/orphion-tray" "$APPS/$TRAY_ENTRY" "$AUTOSTART_DIR/$TRAY_ENTRY"
fi

echo "==> Installing binaries to $BIN"
install -Dm755 "$SRC/orphion" "$SRC/orphionctl" -t "$BIN"

# Desktop sessions often do not have ~/.local/bin in PATH, so use absolute paths.
install_desktop() { # <source> <destination dir> <command>
    install -d "$2"
    sed "s|^Exec=.*|Exec=$BIN/$3|" "packaging/linux/$1" > "$2/$1"
    echo "    $2/$1"
}

echo "==> Installing icon"
install -Dm644 assets/orphion.png "$DATA/icons/hicolor/256x256/apps/orphion.png"

echo "==> Installing the menu entry"
install_desktop io.github.orphion.Orphion.desktop "$APPS" orphion

if $AUTOSTART; then
    echo "==> Enabling tray autostart"
    install_desktop "$TRAY_ENTRY" "$AUTOSTART_DIR" "orphion --tray"
    # Start it now too. If a tray is already running, this one exits.
    echo "==> Starting the tray"
    nohup "$BIN/orphion" --tray >/dev/null 2>&1 &
    disown
fi

command -v update-desktop-database >/dev/null && update-desktop-database "$APPS" >/dev/null 2>&1 || true
echo "Done. Launch \"Orphion\" from the application menu, or run: $BIN/orphion"
