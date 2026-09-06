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
[ ! -d "$APP" ] || mv "$APP" "$HOME/Applications/Soju.previous-$(date +%s).app"
ditto "$ROOT/dist/Soju.app" "$APP"
open "$APP"
