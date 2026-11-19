#!/bin/bash
# Publish the notarized build as GitHub release v<VERSION> and move everything
# that points at "the current version" with it:
#   scripts/publish-release.sh <notes-file>
# Before: merge to main, scripts/build-app.sh (Developer ID), scripts/notarize-app.sh.
# Does: gh release create -> Homebrew tap -> scripts/release-health.py.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
NOTES="${1:?notes file (one bullet per change)}"
[ -s "$NOTES" ] || { echo "Empty notes file: $NOTES"; exit 64; }
VERSION=$(cat VERSION)
REPO="${SOJU_REPO:-BCD1210/soju}"
ZIP="dist/Soju-$VERSION-macos-arm64.zip"
[ -f "$ZIP" ] && [ -f "$ZIP.sha256" ] || { echo "Build and notarize first: $ZIP is missing"; exit 1; }
(cd dist && shasum -a 256 -c "$(basename "$ZIP").sha256" >/dev/null) || { echo "$ZIP does not match its .sha256"; exit 1; }
xcrun stapler validate dist/Soju.app >/dev/null 2>&1 || { echo "dist/Soju.app is not notarized (scripts/notarize-app.sh)"; exit 1; }
[ "$(git rev-parse --abbrev-ref HEAD)" = main ] || { echo "Release from main"; exit 1; }
[ -z "$(git status --porcelain --untracked-files=no)" ] || { echo "Uncommitted changes"; exit 1; }
git fetch -q origin main
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] || { echo "main is not what is on GitHub"; exit 1; }
if gh release view "v$VERSION" -R "$REPO" >/dev/null 2>&1; then
  echo "Release v$VERSION exists already; continuing with the tap and the checks"
else
  gh release create "v$VERSION" "$ZIP" "$ZIP.sha256" -R "$REPO" --target "$(git rev-parse HEAD)" \
    --title "Soju $VERSION" --notes-file "$NOTES"
fi
bash scripts/update-homebrew-tap.sh "$VERSION"
python3 scripts/release-health.py --wait 120
