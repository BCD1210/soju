#!/usr/bin/env bash
# soju steam-gptk on|off|status
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

case "${1:-status}" in
  on)
    [ -f "$PLAIN/$CLIENT" ] || { echo "soju: install Steam first (Platforms, or soju steam-install)" >&2; exit 1; }
    if steam_running; then echo "soju: quit Steam first (soju steam-kill)" >&2; exit 1; fi
    bash "$ROOT/scripts/steam-gptk-engine.sh" >/dev/null || exit 1
    if [ ! -f "$GPTK/$CLIENT" ]; then
      echo "Making the Steam bottle for GPTK (a copy that takes no extra space)"
      rm -rf "$GPTK.new"
      cp -Rc "$PLAIN" "$GPTK.new" 2>/dev/null || { rm -rf "$GPTK.new"; echo "soju: could not copy the Steam bottle" >&2; exit 1; }
      # The copy belongs to the other engine now; drop marks that only the Wine 11 setup reads.
      rm -f "$GPTK.new/.soju-steam-games" "$GPTK.new/.soju-auto-profiles.json"
      mv "$GPTK.new" "$GPTK"
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
  *) echo "usage: soju steam-gptk on|off|status" >&2; exit 64 ;;
esac
