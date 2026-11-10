#!/usr/bin/env bash
# Empacota o binário gnu (GTK via dlopen) num AppImage tipo 2.
# Não executa o GuiaLar. Requer appimagetool no PATH.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${BIN:-$ROOT/target/release/guialar}"
OUT_DIR="${OUT_DIR:-$ROOT/dist}"
NAME="${NAME:-guialar-0.3.0-x86_64.AppImage}"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

test -x "$BIN" || { echo "falta binário gnu: $BIN" >&2; exit 1; }
command -v appimagetool >/dev/null || { echo "falta appimagetool" >&2; exit 1; }

APPDIR="$STAGE/AppDir"
mkdir -p "$APPDIR/usr/bin"
cp "$ROOT/packaging/AppDir/AppRun" "$APPDIR/AppRun"
chmod +x "$APPDIR/AppRun"
cp "$ROOT/packaging/AppDir/guialar.desktop" "$APPDIR/guialar.desktop"
cp "$ROOT/assets/guialar.png" "$APPDIR/guialar.png"
cp "$ROOT/assets/guialar.svg" "$APPDIR/guialar.svg"
cp "$BIN" "$APPDIR/usr/bin/guialar"
chmod +x "$APPDIR/usr/bin/guialar"

mkdir -p "$OUT_DIR"
ARCH=x86_64 appimagetool "$APPDIR" "$OUT_DIR/$NAME"
echo "OK $OUT_DIR/$NAME"
