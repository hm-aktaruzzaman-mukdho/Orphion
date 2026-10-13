#!/usr/bin/env bash
# Builds portable Linux binaries and packs them into a tarball.
#
# The binaries are linked against glibc 2.31 so they run on every mainstream
# distribution from 2020 on (Ubuntu 20.04, Debian 11, Fedora 32, …), not just
# on the one they were built on. Everything graphical (OpenGL, Wayland, X11)
# is loaded at runtime, so glibc is the only hard dependency.
#
# Needs once:  cargo install --locked cargo-zigbuild
#              and zig: a distribution package, or `pip install ziglang`
#              (a venv in research/tools/zig-venv is picked up automatically)
#
# Output: dist/orphion-<version>-linux-x64.tar.gz
set -euo pipefail

cd "$(dirname "$0")/.."
TARGET=x86_64-unknown-linux-gnu
GLIBC=2.31
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
NAME="orphion-$VERSION-linux-x64"
OUT="dist/$NAME"

if [[ -x research/tools/zig-venv/bin/python ]]; then
    PATH="$PWD/research/tools/zig-venv/bin:$PATH"
fi

echo "==> Building $NAME (glibc $GLIBC baseline)"
cargo zigbuild --release --workspace --target "$TARGET.$GLIBC"

rm -rf "$OUT" "$OUT.tar.gz"
mkdir -p "$OUT/bin" "$OUT/assets" "$OUT/packaging/linux"
cp "target/$TARGET/release/"{orphion,orphion-tray,orphionctl} "$OUT/bin/"
cp assets/orphion.png "$OUT/assets/"
# Same layout as the source tree, so install.sh works unchanged in both.
cp packaging/linux/{install.sh,uninstall.sh,io.github.orphion.Orphion.desktop,io.github.orphion.Orphion.Tray.desktop} "$OUT/packaging/linux/"
cp packaging/linux/README.txt "$OUT/"

tar -C dist -czf "$OUT.tar.gz" "$NAME"
echo "==> $OUT.tar.gz"
ls -lh "$OUT.tar.gz"
