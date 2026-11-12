#!/usr/bin/env bash
# The native app passes arguments directly, never shell-interpolated user input.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BASE="${SOJU_BASE:-$HOME/.battlenet-macos}"
export SOJU_BASE="$BASE"
ACTION="${1:-}"; shift || true
case "$ACTION" in
  service) exec python3 "$ROOT/scripts/library-service.py" "$@" ;;
  game-install) exec python3 "$ROOT/scripts/game-library.py" install "${1:?Choose a game}" ;;
  library) exec python3 "$ROOT/scripts/game-library.py" list ;;
  game) exec python3 "$ROOT/scripts/game-library.py" launch "${1:?Choose a game}" ;;
  game-doctor) exec python3 "$ROOT/scripts/game-library.py" diagnose "${1:?Choose a game}" ;;
  game-settings) exec python3 "$ROOT/scripts/game-settings.py" "$@" ;;
  game-storage) exec python3 "$ROOT/scripts/game-storage.py" "$@" ;;
  doctor) exec bash "$ROOT/scripts/doctor.sh" "$@" ;;
  steam-gptk) exec bash "$ROOT/scripts/steam-gptk.sh" "${1:?on or off}" ;;
  report) exec python3 "$ROOT/scripts/report.py" "${1:-}" ;;
  self-update)
    # Detached, so it outlives the app it is about to replace.
    nohup /bin/bash "$ROOT/scripts/self-update.sh" "${1:?version}" "${2:?app}" "${3:?pid}" "${4:?state}" \
      >>"$BASE/logs/self-update.log" 2>&1 </dev/null &
    exit 0 ;;
  launch)
    case "${1:-}" in battlenet|steam|epic|gog|ea|rockstar|ubisoft) exec bash "$ROOT/scripts/play.sh" "$1" ;;
      *) echo "Choose one of the launchers."; exit 64 ;; esac ;;
  vr)
    case "${1:-}" in setup|play|status) exec python3 "$ROOT/scripts/d2r-vr.py" "$1" ;;
      flat) exec python3 "$ROOT/scripts/d2r-vr.py" play --flat ;;
      *) echo "Unknown VR action."; exit 64 ;; esac ;;
  tv)
    case "${1:-}" in setup|start|stop|status) exec python3 "$ROOT/scripts/stream_tv.py" "$1" ;;
      pin) exec python3 "$ROOT/scripts/stream_tv.py" pin "${2:?Enter the PIN}" ;;
      unpair) exec python3 "$ROOT/scripts/stream_tv.py" unpair "${2:?Choose a device}" ;;
      *) echo "Unknown TV action."; exit 64 ;; esac ;;
  didim)
    case "${1:-}" in install) exec python3 "$ROOT/scripts/didim_games.py" install "${2:?Choose the game zip}" ;;
      remove) exec python3 "$ROOT/scripts/didim_games.py" remove "${2:?Choose a game}" ;;
      activate) exec python3 "$ROOT/scripts/didim_games.py" activate "${2:?Choose a game}" ;;
      demo) exec python3 "$ROOT/scripts/didim_games.py" demo "${2:?Choose a game}" ;;
      *) echo "Unknown Didim Labs action."; exit 64 ;; esac ;;
  xbox)
    case "${1:-}" in login|logout|kill|status) exec bash "$ROOT/scripts/xbox.sh" "$1" ;;
      location) exec bash "$ROOT/scripts/xbox.sh" location "${2:?Choose a folder}" ;;
      *) echo "Unknown Xbox action."; exit 64 ;; esac ;;
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
      echo "Install Battle.net, Epic, GOG, EA, Rockstar or Ubisoft first to prepare its engine, then add GPTK."; exit 1;
    }
    # With a folder the user picked, install from it; without one, download the payload.
    if [ -n "${1:-}" ]; then exec bash "$ROOT/scripts/get-gptk.sh" "$1"; fi
    exec python3 "$ROOT/scripts/fetch-gptk.py" ;;
  update)
    echo "Soju $(cat "$ROOT/VERSION"): component update"
    echo "App updates: https://github.com/BCD1210/soju/releases"
    result=0
    if [ -x "$BASE/cx26-engine/bin/wine" ]; then
      SOJU_UPDATE_ENGINE=1 bash "$ROOT/scripts/update.sh" --scripts-updated || result=1
    fi
    if [ -f "$BASE/steam-bottle/drive_c/Program Files (x86)/Steam/steam.exe" ]; then
      bash "$ROOT/scripts/setup-steam-games.sh" || result=1
    fi
    # A changed backend recipe rebuilds Xodus; an unchanged one is only checked.
    if [ -d "$BASE/xodus" ] && [ -z "${SOJU_XODUS_BIN:-}" ]; then
      python3 "$ROOT/scripts/fetch-xodus-backend.py" || result=1
    fi
    echo "Component update finished."
    exit "$result" ;;
  *) echo "Unknown desktop action: $ACTION"; exit 64 ;;
esac
