#!/usr/bin/env bash
# Builds the project and installs it for the current user (no root needed).
#
#   packaging/install.sh              install qcy-gui, qcy-tray and qcyctl
#   packaging/install.sh --autostart  additionally start the tray on login
#
# PREFIX defaults to ~/.local (binaries in ~/.local/bin).
set -euo pipefail

cd "$(dirname "$0")/.."
PREFIX="${PREFIX:-$HOME/.local}"
BIN="$PREFIX/bin"
APPS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
AUTOSTART_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/autostart"

echo "==> Building (release)…"
cargo build --release --workspace

echo "==> Installing binaries to $BIN"
install -Dm755 target/release/qcy-gui target/release/qcy-tray target/release/qcyctl -t "$BIN"

# Desktop sessions often do not have ~/.local/bin in PATH, so use absolute paths.
install_desktop() { # <source> <destination dir> <binary>
    install -d "$2"
    sed "s|^Exec=.*|Exec=$BIN/$3|" "packaging/$1" > "$2/$1"
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
