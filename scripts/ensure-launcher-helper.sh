#!/usr/bin/env bash
# Repair a tray-restore helper independently of the official launcher setup.
# Optional support directory allows isolated builds and regression tests.
set -euo pipefail
MODE="${1:?usage: ensure-launcher-helper.sh epic|gog [support-directory]}"
case "$MODE" in epic|gog) ;; *) echo "Unknown launcher: $MODE" >&2; exit 1;; esac
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SUPPORT="${2:-$HOME/.battlenet-macos/$MODE-support}"
DEST="$SUPPORT/soju-$MODE-restore.exe"
[ ! -s "$DEST" ] || exit 0
mkdir -p "$SUPPORT"
echo "==> Building the $MODE tray-restore helper (mingw-w64)"
command -v x86_64-w64-mingw32-gcc >/dev/null || brew install mingw-w64
TMP=$(mktemp "$SUPPORT/.soju-$MODE-restore.XXXXXX")
trap 'rm -f "$TMP"' EXIT
x86_64-w64-mingw32-gcc -O2 -Wall -mwindows \
  -o "$TMP" "$ROOT/tools/soju-$MODE-restore.c" -lpsapi
[ -s "$TMP" ] || { echo "Compiler produced an empty helper" >&2; exit 1; }
mv -f "$TMP" "$DEST"
