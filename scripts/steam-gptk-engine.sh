#!/usr/bin/env bash
# Prepare the engine Steam runs on when "Steam with Apple GPTK" is on, and print
# its path. Verified 2026-11-11 on an M4 Pro, macOS 26.5: the Steam client signs
# in and downloads, and Overwatch 2 from Steam reaches its main menu online.
#
# It is a copy of the CX engine (an APFS clone: no extra disk space) with one
# file replaced: winemac.so with patches/winemac-cross-process-swapchains.patch
# from the launcher-support component (scripts/build-launcher-support.sh).
# Steam's browser UI draws from a helper process into a window steam.exe owns;
# the stock driver leaves that window black. Everything else, Apple's d3d11,
# d3d12, dxgi, atidxx64 and nvapi64 shims included, stays as in the CX engine,
# so Steam games get D3DMetal the way Battle.net games do. (The launcher engine
# for EA, Rockstar and Ubisoft also swaps D3D11 for DXMT; games here want
# D3DMetal, and Overwatch 2 stops at start without Apple's AMD shim.)
#
# The copy is rebuilt when the CX engine, its GPTK payload or the component
# changes, never while Steam runs on it.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASE="${SOJU_BASE:-$HOME/.battlenet-macos}"
SRC="${SOJU_CX_ENGINE:-$BASE/cx26-engine}"
DEST="${SOJU_STEAM_GPTK_ENGINE:-$BASE/cx26-engine-steam}"

[ -x "$SRC/bin/wine" ] || { echo "soju: the engine is not installed; run: soju install" >&2; exit 1; }
[ -f "$SRC/lib/external/libd3dshared.dylib" ] || { echo "soju: Apple GPTK is not installed; run: soju gptk" >&2; exit 1; }
SUPPORT=$(SOJU_BASE="$BASE" python3 "$ROOT/scripts/fetch-launcher-support.py") || exit 1

engine_id="$(cat "$SRC/.soju-engine-release" 2>/dev/null || /usr/bin/stat -f %m "$SRC/bin/wine")"
gptk_id="$(/usr/bin/stat -f %m "$SRC/lib/external/libd3dshared.dylib")"
want="$engine_id|$gptk_id|$(cat "$SUPPORT/.soju-support-version")"

if [ "$(cat "$DEST/.soju-steam-engine" 2>/dev/null)" != "$want" ]; then
  # Wine starts its server as <engine>/lib/wine/../../bin/wineserver.
  if pgrep -f "$DEST/(lib/wine/\.\./\.\./)?bin/wineserver" >/dev/null 2>&1; then
    echo "soju: quit Steam first (soju steam-kill)" >&2
    exit 1
  fi
  echo "Preparing the Steam engine with Apple GPTK" >&2
  rm -rf "$DEST.new"
  cp -Rc "$SRC" "$DEST.new" 2>/dev/null || { rm -rf "$DEST.new"; cp -R "$SRC" "$DEST.new"; }
  cp -f "$SUPPORT/winemac.so" "$DEST.new/lib/wine/x86_64-unix/winemac.so"
  printf '%s\n' "$want" > "$DEST.new/.soju-steam-engine"
  rm -rf "$DEST"
  mv "$DEST.new" "$DEST"
fi
echo "$DEST"
