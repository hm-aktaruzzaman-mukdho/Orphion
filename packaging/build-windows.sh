#!/usr/bin/env bash
# Cross-compiles the Windows version on Linux and packs it into a zip.
#
# Needs once:  rustup target add x86_64-pc-windows-gnu
#              sudo apt install mingw-w64   (the linker; often already present)
#
# Output: dist/orphion-<version>-windows-x64.zip
set -euo pipefail

cd "$(dirname "$0")/.."
TARGET=x86_64-pc-windows-gnu
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
NAME="orphion-$VERSION-windows-x64"
OUT="dist/$NAME"

echo "==> Building $NAME"
cargo build --release --workspace --target "$TARGET"

rm -rf "$OUT" "$OUT.zip"
mkdir -p "$OUT"
cp "target/$TARGET/release/"{orphion,orphion-tray,orphionctl}.exe "$OUT/"
cp packaging/windows/{install.ps1,uninstall.ps1,README.txt} "$OUT/"
# Windows tools expect CRLF line endings in text files.
for f in "$OUT"/*.ps1 "$OUT/README.txt"; do
    sed 's/$/\r/' "$f" > "$f.crlf" && mv "$f.crlf" "$f"
done

(cd dist && zip -qr "$NAME.zip" "$NAME")
echo "==> dist/$NAME.zip"
ls -lh "dist/$NAME.zip"
