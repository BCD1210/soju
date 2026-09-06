#!/usr/bin/env bash
# Native Apple Silicon app; build outputs never contain Wine, games or Apple GPTK.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${SOJU_APP_OUTPUT:-$ROOT/dist}"
VERSION=$(cat "$ROOT/VERSION")
mkdir -p "$OUT"
STAGE=$(mktemp -d "$OUT/.app-build.XXXXXX")
trap 'rm -rf "$STAGE"' EXIT
APP="$STAGE/Soju.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources/soju"
swiftc -swift-version 5 -parse-as-library -O -target arm64-apple-macosx14.0 \
  "$ROOT/app/Soju.swift" "$ROOT/app/Library.swift" -o "$APP/Contents/MacOS/Soju"
for dir in scripts tools patches third_party resources; do
  ditto "$ROOT/$dir" "$APP/Contents/Resources/soju/$dir"
done
for file in install.sh VERSION LICENSE NOTICE; do
  cp "$ROOT/$file" "$APP/Contents/Resources/soju/"
done
find "$APP" -name __pycache__ -type d -prune -exec rm -rf {} +
python3 - "$APP" "$VERSION" <<'PY'
import plistlib, sys
from pathlib import Path
p=Path(sys.argv[1])/"Contents/Info.plist"
p.write_bytes(plistlib.dumps({
    "CFBundleName": "Soju", "CFBundleDisplayName": "Soju",
    "CFBundleIdentifier": "app.soju.desktop", "CFBundleExecutable": "Soju",
    "CFBundlePackageType": "APPL", "CFBundleShortVersionString": sys.argv[2],
    "CFBundleVersion": sys.argv[2], "LSMinimumSystemVersion": "14.0",
    "NSHighResolutionCapable": True,
    "NSHumanReadableCopyright": "Soju contributors — GPL-3.0-or-later",
    "LSApplicationCategoryType": "public.app-category.games"
}))
PY
swift "$ROOT/app/Icon.swift" "$STAGE"
iconutil -c icns "$STAGE/Soju.iconset" -o "$APP/Contents/Resources/Soju.icns"
/usr/libexec/PlistBuddy -c "Add :CFBundleIconFile string Soju" "$APP/Contents/Info.plist"
codesign --force --sign - "$APP"
codesign --verify --deep --strict "$APP"
[ ! -d "$OUT/Soju.app" ] || rm -rf "$OUT/Soju.app"
mv "$APP" "$OUT/Soju.app"
ditto -c -k --sequesterRsrc --keepParent "$OUT/Soju.app" "$OUT/Soju-$VERSION-macos-arm64.zip"
(cd "$OUT" && shasum -a 256 "Soju-$VERSION-macos-arm64.zip" > "Soju-$VERSION-macos-arm64.zip.sha256")
echo "Built $OUT/Soju.app"
