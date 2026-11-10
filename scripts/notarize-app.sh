#!/usr/bin/env bash
# Release step after a Developer ID build: submit dist/Soju-<version>-macos-arm64.zip to Apple's
# notary service, staple the ticket to the app and rebuild the zip and its checksum.
#
#   SOJU_SIGN_IDENTITY="Developer ID Application: ..." bash scripts/build-app.sh
#   SOJU_NOTARY_KEY=AuthKey_XXXX.p8 SOJU_NOTARY_KEY_ID=XXXX SOJU_NOTARY_ISSUER=<uuid> bash scripts/notarize-app.sh
#
# The key is an App Store Connect API key of the signing team. Nothing here is stored in the repo.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${SOJU_APP_OUTPUT:-$ROOT/dist}"
VERSION=$(cat "$ROOT/VERSION")
ZIP="$OUT/Soju-$VERSION-macos-arm64.zip"
APP="$OUT/Soju.app"
: "${SOJU_NOTARY_KEY:?path to the App Store Connect API key (.p8)}"
: "${SOJU_NOTARY_KEY_ID:?App Store Connect API key id}"
: "${SOJU_NOTARY_ISSUER:?App Store Connect issuer id}"
[ -f "$ZIP" ] && [ -d "$APP" ] || { echo "Build first: $ZIP and $APP are missing"; exit 1; }
# Read the signature into a variable first: with pipefail, grep -q closing the pipe early fails the check.
signature=$(codesign -dv "$APP" 2>&1 || true)
case "$signature" in
  *"(runtime)"*) ;;
  *) echo "$APP is not a Developer ID build (set SOJU_SIGN_IDENTITY for scripts/build-app.sh)"; exit 1 ;;
esac

result=$(xcrun notarytool submit "$ZIP" --key "$SOJU_NOTARY_KEY" --key-id "$SOJU_NOTARY_KEY_ID" \
  --issuer "$SOJU_NOTARY_ISSUER" --wait --output-format json)
status=$(printf '%s' "$result" | plutil -extract status raw -o - -)
if [ "$status" != "Accepted" ]; then
  id=$(printf '%s' "$result" | plutil -extract id raw -o - - 2>/dev/null || true)
  echo "Notarization: $status"
  [ -z "$id" ] || xcrun notarytool log "$id" --key "$SOJU_NOTARY_KEY" --key-id "$SOJU_NOTARY_KEY_ID" --issuer "$SOJU_NOTARY_ISSUER"
  exit 1
fi
xcrun stapler staple "$APP"
rm -f "$ZIP" "$ZIP.sha256"
ditto -c -k --sequesterRsrc --keepParent "$APP" "$ZIP"
(cd "$OUT" && shasum -a 256 "$(basename "$ZIP")" > "$(basename "$ZIP").sha256")
spctl -a -vv -t exec "$APP"
echo "Notarized $ZIP"
