#!/usr/bin/env bash
# Installs QCY Earbuds for the current user (no root needed). From the source
# tree it builds first; from a release tarball (made by build-linux.sh) it
# installs the prebuilt binaries in bin/.
#
#   packaging/linux/install.sh              install qcy-gui, qcy-tray and qcyctl
#   packaging/linux/install.sh --autostart  additionally start the tray on login
#
# PREFIX defaults to ~/.local (binaries in ~/.local/bin).
set -euo pipefail

# Work from the repository (or tarball) root, two levels up.
cd "$(dirname "$0")/../.."
PREFIX="${PREFIX:-$HOME/.local}"
BIN="$PREFIX/bin"
APPS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
AUTOSTART_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/autostart"

if [[ -x bin/qcy-gui && -x bin/qcy-tray && -x bin/qcyctl ]]; then
    SRC=bin
else
    echo "==> Building (release)…"
    cargo build --release --workspace
    SRC=target/release
fi

echo "==> Installing binaries to $BIN"
install -Dm755 "$SRC/qcy-gui" "$SRC/qcy-tray" "$SRC/qcyctl" -t "$BIN"

# Desktop sessions often do not have ~/.local/bin in PATH, so use absolute paths.
install_desktop() { # <source> <destination dir> <binary>
    install -d "$2"
    sed "s|^Exec=.*|Exec=$BIN/$3|" "packaging/linux/$1" > "$2/$1"
    echo "    $2/$1"
}

echo "==> Installing icon"
install -Dm644 assets/qcy-earbuds.png "${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/256x256/apps/qcy-earbuds.png"

echo "==> Installing desktop entries"
install_desktop io.github.qcy_linux.Control.desktop "$APPS" qcy-gui
install_desktop io.github.qcy_linux.Tray.desktop "$APPS" qcy-tray

if [[ "${1:-}" == "--autostart" ]]; then
    echo "==> Enabling tray autostart"
    install_desktop io.github.qcy_linux.Tray.desktop "$AUTOSTART_DIR" qcy-tray
fi

command -v update-desktop-database >/dev/null && update-desktop-database "$APPS" >/dev/null 2>&1 || true
echo "Done. Launch \"QCY Earbuds\" from the application menu, or run: $BIN/qcy-gui"
