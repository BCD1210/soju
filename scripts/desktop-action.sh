#!/usr/bin/env bash
# The native app passes arguments directly, never shell-interpolated user input.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BASE="${SOJU_BASE:-$HOME/.battlenet-macos}"
export SOJU_BASE="$BASE"
ACTION="${1:-}"; shift || true
case "$ACTION" in
  doctor) exec bash "$ROOT/scripts/doctor.sh" "$@" ;;
  launch)
    case "${1:-}" in battlenet|steam|epic|gog) exec bash "$ROOT/scripts/play.sh" "$1" ;;
      *) echo "Choose one of the four launchers."; exit 64 ;; esac ;;
  install)
    export SOJU_PLATFORMS="${1:?Choose at least one launcher}"
    if ! xcode-select -p >/dev/null 2>&1; then
      echo "Install Apple's Command Line Tools first: xcode-select --install"; exit 1
    fi
    if ! command -v brew >/dev/null; then
      echo "Install Homebrew from https://brew.sh, then retry."; exit 1
    fi
    # Shortcuts point at a stable managed copy even if the downloaded app moves.
    MANAGED="$BASE/desktop-scripts"
    mkdir -p "$MANAGED"
    ditto "$ROOT" "$MANAGED"
    exec bash "$MANAGED/install.sh" ;;
  gptk)
    [ -x "$BASE/cx26-engine/bin/wine" ] || {
      echo "Install Battle.net, Epic or GOG first to prepare its engine, then add GPTK."; exit 1;
    }
    exec bash "$ROOT/scripts/get-gptk.sh" "${1:?Select the mounted toolkit}" ;;
  update)
    echo "Soju $(cat "$ROOT/VERSION") — component update"
    echo "App updates: https://github.com/BCD1210/soju/releases"
    result=0
    if [ -x "$BASE/cx26-engine/bin/wine" ]; then
      SOJU_UPDATE_ENGINE=1 bash "$ROOT/scripts/update.sh" --scripts-updated || result=1
    fi
    if [ -f "$BASE/steam-bottle/drive_c/Program Files (x86)/Steam/steam.exe" ]; then
      bash "$ROOT/scripts/setup-steam-games.sh" || result=1
    fi
    echo "Component update finished."
    exit "$result" ;;
  *) echo "Unknown desktop action: $ACTION"; exit 64 ;;
esac
