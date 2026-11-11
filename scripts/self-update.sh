#!/usr/bin/env bash
# Replace the running Soju.app with a newer release, called by the app's update card.
#   self-update.sh <version> <path to Soju.app> <pid of the running app> <state file>
# Downloads Soju-<version>-macos-arm64.zip from the GitHub release, checks its SHA-256
# against the release's .sha256 file, then accepts the app only if it is signed by
# Didim Labs' Developer ID (team SPKW5D5A3P), notarized (Gatekeeper accepts it) and
# carries that version. Writes "ready" to the state file and waits for the app to quit,
# swaps the bundle (the old one is kept until the new one is in place) and reopens it.
# Any failure writes "failed: <reason>" and leaves the installed app untouched.
set -euo pipefail
# Run from a copy: this file lives inside the bundle that is about to be replaced.
if [ -z "${SOJU_SELF_UPDATE_COPY:-}" ]; then
  copy=$(mktemp "${TMPDIR:-/tmp}/soju-self-update.XXXXXX")
  cp "$0" "$copy"
  SOJU_SELF_UPDATE_COPY="$copy" exec /bin/bash "$copy" "$@"
fi
trap 'rm -f "$SOJU_SELF_UPDATE_COPY"' EXIT
VERSION="${1:?version}"; APP="${2:?app path}"; APP_PID="${3:?pid}"; STATE="${4:?state file}"
TEAM="SPKW5D5A3P"
REL="https://github.com/BCD1210/soju/releases/download/v$VERSION"
ZIP="Soju-$VERSION-macos-arm64.zip"
fail(){ printf 'failed: %s\n' "$*" > "$STATE"; echo "self-update: $*" >&2; exit 1; }

[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "bad version"
[[ "$APP" == *.app && -d "$APP" ]] || fail "app not found"
case "$APP" in */AppTranslocation/*) fail "Move Soju to Applications first, then update" ;; esac
[ -w "$(dirname "$APP")" ] && [ -w "$APP" ] || fail "no permission to replace $APP"

WORK=$(mktemp -d "${TMPDIR:-/tmp}/soju-update.XXXXXX")
trap 'rm -rf "$WORK" "$SOJU_SELF_UPDATE_COPY"' EXIT
printf 'downloading\n' > "$STATE"
curl -fsSL --proto =https --retry 3 "$REL/$ZIP" -o "$WORK/$ZIP" || fail "download failed"
curl -fsSL --proto =https --retry 3 "$REL/$ZIP.sha256" -o "$WORK/sum" || fail "checksum download failed"
want=$(awk '{print $1}' "$WORK/sum"); got=$(shasum -a 256 "$WORK/$ZIP" | awk '{print $1}')
[ -n "$want" ] && [ "$want" = "$got" ] || fail "checksum mismatch"
mkdir "$WORK/x" && ditto -x -k "$WORK/$ZIP" "$WORK/x" || fail "could not unpack"
NEW="$WORK/x/Soju.app"
[ -d "$NEW" ] || fail "no Soju.app in the download"
codesign --verify --deep --strict "$NEW" 2>/dev/null || fail "signature does not verify"
codesign -dv "$NEW" 2>&1 | grep -qx "TeamIdentifier=$TEAM" || fail "not signed by Didim Labs"
spctl -a -t exec "$NEW" 2>/dev/null || fail "Gatekeeper does not accept it"
v=$(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' "$NEW/Contents/Info.plist" 2>/dev/null || true)
[ "$v" = "$VERSION" ] || fail "version is $v, expected $VERSION"

printf 'ready\n' > "$STATE"
for _ in $(seq 1 120); do kill -0 "$APP_PID" 2>/dev/null || break; sleep 0.5; done
kill -0 "$APP_PID" 2>/dev/null && fail "Soju did not quit"
OLD="$APP.old-$$"
mv "$APP" "$OLD" || fail "could not move the old app aside"
if ditto "$NEW" "$APP"; then
  rm -rf "$OLD"
  printf 'done %s\n' "$VERSION" > "$STATE"
  open "$APP"
else
  rm -rf "$APP"; mv "$OLD" "$APP"
  fail "could not copy the new app; the old one is back"
fi
