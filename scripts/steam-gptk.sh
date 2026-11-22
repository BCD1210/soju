#!/usr/bin/env bash
# soju steam-gptk on|off|status|play <steam url>
#
# "Steam with Apple GPTK": Steam and its games run on the CX engine with Apple's
# Game Porting Toolkit (scripts/steam-gptk-engine.sh) instead of the private
# Wine 11 + DXMT stack. Needed for D3D12 games and for Blizzard games sold on
# Steam: Overwatch 2 reaches its main menu this way, and Diablo II: Resurrected's
# loader needs GPTK's libd3dshared to get through Rosetta.
#
# It uses its own bottle, steam-gptk-bottle, made the first time as an APFS
# clone of steam-bottle: installed games show up without downloading again and
# take no extra space until either side changes them. Steam asks you to sign in
# once in it. Turning it off goes back to steam-bottle, still signed in; the
# GPTK bottle is kept, so turning it on again needs no new sign-in.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASE="${SOJU_BASE:-$HOME/.battlenet-macos}"
FLAG="$BASE/steam-gptk"
PLAIN="$BASE/steam-bottle"
GPTK="$BASE/steam-gptk-bottle"
CLIENT="drive_c/Program Files (x86)/Steam/steam.exe"
source "$ROOT/scripts/steam-runtime.sh"

steam_running(){
  pgrep -f "$BASE/cx26-engine-steam/(lib/wine/\.\./\.\./)?bin/wineserver" >/dev/null 2>&1 && return 0
  ! python3 "$ROOT/scripts/steam-session.py" idle "$STEAM_WINE_ROOT" >/dev/null 2>&1
}

# Games installed in the plain bottle since the GPTK bottle was made: bring them over (APFS clones, no extra
# space), so the library and Steam show the same games in both modes and nothing is downloaded twice.
sync_games(){
  local apps="drive_c/Program Files (x86)/Steam/steamapps" f name dir
  [ -d "$PLAIN/$apps" ] && [ -d "$GPTK/$apps" ] || return 0
  for f in "$PLAIN/$apps"/appmanifest_*.acf; do
    [ -f "$f" ] || continue
    name=$(basename "$f")
    [ ! -e "$GPTK/$apps/$name" ] || continue
    dir=$(sed -n 's/^[[:space:]]*"installdir"[[:space:]]*"\(.*\)"[[:space:]]*$/\1/p' "$f" | head -1)
    [ -n "$dir" ] && [ -d "$PLAIN/$apps/common/$dir" ] || continue
    case "$dir" in */*|*\\*|.|..) continue ;; esac
    mkdir -p "$GPTK/$apps/common"
    if [ ! -e "$GPTK/$apps/common/$dir" ]; then
      cp -Rc "$PLAIN/$apps/common/$dir" "$GPTK/$apps/common/$dir" 2>/dev/null || { rm -rf "$GPTK/$apps/common/$dir"; continue; }
    fi
    cp -c "$f" "$GPTK/$apps/$name" 2>/dev/null || cp "$f" "$GPTK/$apps/$name"
    echo "Brought over to the GPTK bottle: $dir"
  done
}

case "${1:-status}" in
  play)   # Play for a game that needs Direct3D 12: whatever is missing for the GPTK mode is done first, then the game starts.
    shift
    if [ "$(bash "${BASH_SOURCE[0]}" status)" != on ]; then
      echo "This game needs Direct3D 12, so Steam switches to its Apple GPTK mode. This happens once; your other Steam games stay."
      if [ ! -x "$BASE/cx26-engine/bin/wine" ] || [ ! -f "$BASE/cx26-engine/lib/external/libd3dshared.dylib" ]; then
        echo "Downloading what that mode needs (the engine and Apple's Game Porting Toolkit, about 600 MB, once)"
        SOJU_PLATFORMS=engine SOJU_NONINTERACTIVE=1 bash "$ROOT/install.sh" </dev/null \
          || { echo "soju: the Direct3D 12 engine could not be installed. Check your connection and press Play again." >&2; exit 1; }
        [ -f "$BASE/cx26-engine/lib/external/libd3dshared.dylib" ] \
          || { echo "soju: Apple's Game Porting Toolkit could not be downloaded. In Stores, open Settings, Apple GPTK, Download and install, then press Play again." >&2; exit 1; }
      fi
      if steam_running; then
        echo "Closing Steam to switch"
        bash "$ROOT/scripts/play.sh" steam-kill >/dev/null 2>&1 || true
        for _ in $(seq 1 30); do steam_running || break; sleep 1; done
      fi
      bash "${BASH_SOURCE[0]}" on || exit 1
    fi
    exec bash "$ROOT/scripts/play.sh" steam "$@"
    ;;
  on)
    [ -f "$PLAIN/$CLIENT" ] || { echo "soju: install Steam first (Stores, or soju steam-install)" >&2; exit 1; }
    if steam_running; then echo "soju: quit Steam first (soju steam-kill)" >&2; exit 1; fi
    bash "$ROOT/scripts/steam-gptk-engine.sh" >/dev/null || exit 1
    if [ ! -f "$GPTK/$CLIENT" ]; then
      echo "Making the Steam bottle for GPTK (a copy that takes no extra space)"
      rm -rf "$GPTK.new"
      cp -Rc "$PLAIN" "$GPTK.new" 2>/dev/null || { rm -rf "$GPTK.new"; echo "soju: could not copy the Steam bottle" >&2; exit 1; }
      # The copy belongs to the other engine now; drop marks that only the Wine 11 setup reads.
      rm -f "$GPTK.new/.soju-steam-games" "$GPTK.new/.soju-auto-profiles.json"
      mv "$GPTK.new" "$GPTK"
    else
      sync_games
    fi
    touch "$FLAG"
    echo "Steam with Apple GPTK: on. Steam asks you to sign in once the first time."
    ;;
  off)
    if steam_running; then echo "soju: quit Steam first (soju steam-kill)" >&2; exit 1; fi
    rm -f "$FLAG"
    echo "Steam with Apple GPTK: off (the GPTK bottle is kept: $GPTK)"
    ;;
  status)
    if [ -f "$FLAG" ] && [ -f "$GPTK/$CLIENT" ]; then echo on; else echo off; fi
    ;;
  *) echo "usage: soju steam-gptk on|off|status|play <steam url>" >&2; exit 64 ;;
esac
