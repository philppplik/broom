#!/bin/bash
# Usage: make-dmg.sh <universal-binary> <version> <out.dmg>
set -euo pipefail
BIN="$1"; VER="$2"; OUT="$3"
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(mktemp -d)/Broom"
APP="$ROOT/Broom.app/Contents"
mkdir -p "$APP/MacOS" "$APP/Resources"
sed "s/__VERSION__/$VER/g" "$HERE/Info.plist" > "$APP/Info.plist"
cp "$HERE/launcher.sh" "$APP/MacOS/Broom" && chmod 755 "$APP/MacOS/Broom"
cp "$BIN" "$APP/Resources/broom" && chmod 755 "$APP/Resources/broom"
cp "$HERE/Install command-line tool.command" "$ROOT/" && chmod 755 "$ROOT/Install command-line tool.command"
cp "$HERE/../../README.md" "$HERE/../../LICENSE" "$ROOT/"
ln -s /Applications "$ROOT/Applications"
hdiutil create -volname "Broom $VER" -srcfolder "$ROOT" -ov -format UDZO "$OUT"
