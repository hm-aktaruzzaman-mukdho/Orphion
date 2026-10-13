#!/usr/bin/env bash
# Cross-compiles the Windows version on Linux and packs it into a zip.
#
# Needs once:  rustup target add x86_64-pc-windows-gnu
#              sudo apt install mingw-w64 nsis   (the linker and the setup
#                                                 compiler; often present)
#
# Output: dist/orphion-<version>-windows-x64.zip
#         dist/orphion-<version>-windows-x64-setup.exe  (installer: per user,
#                                                        or for everyone with UAC)
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
cp "target/$TARGET/release/"{orphion,orphionctl}.exe "$OUT/"
cp packaging/windows/{install.ps1,uninstall.ps1,README.txt} "$OUT/"
# Windows tools expect CRLF line endings in text files.
for f in "$OUT"/*.ps1 "$OUT/README.txt"; do
    sed 's/$/\r/' "$f" > "$f.crlf" && mv "$f.crlf" "$f"
done

(cd dist && zip -qr "$NAME.zip" "$NAME")
echo "==> dist/$NAME.zip"

echo "==> Building the installer"
if ! command -v makensis >/dev/null; then
    echo "makensis not found: install NSIS (e.g. sudo apt install nsis)" >&2
    exit 1
fi
makensis -V2 -INPUTCHARSET UTF8 -DVERSION="$VERSION" -DROOT="$PWD" -DSRC="$PWD/$OUT" \
    -DOUTFILE="$PWD/dist/$NAME-setup.exe" packaging/windows/orphion.nsi
echo "==> dist/$NAME-setup.exe"
ls -lh "dist/$NAME.zip" "dist/$NAME-setup.exe"
