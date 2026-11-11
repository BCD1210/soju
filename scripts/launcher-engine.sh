#!/usr/bin/env bash
# Prepare the engine the EA app, the Rockstar Games Launcher and Ubisoft Connect
# run on, and print its path. Verified 2026-11-10 on an M4 Pro, macOS 26.5: all
# three install and open their sign-in windows on it.
#
# It is a copy of the CX engine (an APFS clone: no extra disk space) with the
# launcher-support component on top (scripts/build-launcher-support.sh):
#  - winemac.so with patches/winemac-cross-process-swapchains.patch. These
#    launchers draw their browser UI from a GPU process into a window another
#    process owns; the stock driver cannot present that, so the windows stay
#    white (Ubisoft) or black (Rockstar, EA).
#  - DXMT for d3d10core/d3d11/dxgi, which presents across processes when
#    DXMT_ALLOW_CROSS_PROCESS_SWAPCHAIN=1. D3DMetal cannot. D3D12 stays on D3DMetal.
# Battle.net, Epic and GOG keep the CX engine exactly as it is.
#
# The copy is rebuilt when the CX engine, its GPTK payload or the component
# changes, never while one of these launchers is running.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASE="${SOJU_BASE:-$HOME/.battlenet-macos}"
SRC="${SOJU_CX_ENGINE:-$BASE/cx26-engine}"
DEST="${SOJU_LAUNCHER_ENGINE:-$BASE/cx26-engine-launchers}"

[ -x "$SRC/bin/wine" ] || { echo "soju: the engine is not installed; run: soju install" >&2; exit 1; }
SUPPORT=$(SOJU_BASE="$BASE" python3 "$ROOT/scripts/fetch-launcher-support.py") || exit 1

engine_id="$(cat "$SRC/.soju-engine-release" 2>/dev/null || /usr/bin/stat -f %m "$SRC/bin/wine")"
gptk="no-gptk"; [ -f "$SRC/lib/external/libd3dshared.dylib" ] && gptk="gptk"
want="$engine_id|$gptk|$(cat "$SUPPORT/.soju-support-version")"

if [ "$(cat "$DEST/.soju-launcher-engine" 2>/dev/null)" != "$want" ]; then
  # Wine starts its server as <engine>/lib/wine/../../bin/wineserver.
  if pgrep -f "$DEST/(lib/wine/\.\./\.\./)?bin/wineserver" >/dev/null 2>&1; then
    echo "soju: quit the EA app, Rockstar Games Launcher and Ubisoft Connect first (soju ea-kill, rockstar-kill, ubisoft-kill)" >&2
    exit 1
  fi
  echo "Preparing the engine for the EA app, Rockstar and Ubisoft" >&2
  rm -rf "$DEST.new"
  cp -Rc "$SRC" "$DEST.new" 2>/dev/null || { rm -rf "$DEST.new"; cp -R "$SRC" "$DEST.new"; }
  L="$DEST.new/lib/wine"
  cp -f "$SUPPORT/winemac.so" "$L/x86_64-unix/winemac.so"
  for arch in x86_64-windows i386-windows; do
    for f in d3d10core d3d11 dxgi winemetal; do
      rm -f "$L/$arch/$f.dll"; cp "$SUPPORT/dxmt/$arch/$f.dll" "$L/$arch/$f.dll"
    done
  done
  # Apple's unixlib symlinks for these three would send the DXMT DLLs to D3DMetal.
  rm -f "$L/x86_64-unix/d3d10core.so" "$L/x86_64-unix/d3d11.so" "$L/x86_64-unix/dxgi.so"
  cp -f "$SUPPORT/dxmt/x86_64-unix/winemetal.so" "$L/x86_64-unix/winemetal.so"
  printf '%s\n' "$want" > "$DEST.new/.soju-launcher-engine"
  rm -rf "$DEST"
  mv "$DEST.new" "$DEST"
fi
echo "$DEST"
