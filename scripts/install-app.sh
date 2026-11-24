#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
[ -f "$ROOT/app/Soju.swift" ] || {
  echo "Download the desktop app from https://github.com/BCD1210/soju/releases"; exit 1;
}
bash "$ROOT/scripts/build-app.sh"
APP="$HOME/Applications/Soju.app"
mkdir -p "$HOME/Applications"
if pgrep -f '/Soju.app/Contents/MacOS/Soju$' >/dev/null; then
  echo "Quit Soju before replacing the app. Built app: $ROOT/dist/Soju.app"; exit 1
fi
# The app it replaces is kept, once, where Spotlight does not look (a folder ending in .noindex), so typing
# "Soju" in Spotlight finds this one and not a pile of older builds.
PREV="${SOJU_BASE:-$HOME/.battlenet-macos}/previous-app.noindex"
if [ -d "$APP" ]; then rm -rf "$PREV"; mkdir -p "$PREV"; mv "$APP" "$PREV/Soju.app"; fi
ditto "$ROOT/dist/Soju.app" "$APP"
open "$APP"
