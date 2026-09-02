#!/usr/bin/env bash
# Install libd3dshared + D3DMetal.framework from Apple's Game Porting Toolkit
# into the engine.
#
# Why it's needed: libd3dshared is required for D2R's loader (anti-cheat) to get
# through Rosetta 2 (it registers non-native code regions). It also provides the
# graphics backend (D3DMetal). Apple forbids redistributing it, so you download
# it yourself: a free Apple ID is enough.
#
# 1) Download the "Game Porting Toolkit" dmg from
#    https://developer.apple.com/games/game-porting-toolkit/
#    (requires an Apple ID sign-in, free)
# 2) With the dmg mounted (or passing a path), run this script:
#      scripts/get-gptk.sh                  # auto-detect the mounted volume
#      scripts/get-gptk.sh /path/to/GPTK    # explicit path
#
# Alternative: if CrossOver (trial included) is installed, it is extracted from
# there automatically.
set -euo pipefail

ENGINE="${ENGINE:-$HOME/.battlenet-macos/cx26-engine}"
[ -d "$ENGINE/lib" ] || { echo "Engine not found at $ENGINE. Run install.sh (prebuilt) or scripts/build-engine.sh first."; exit 1; }

find_payload() {
  local roots=("$@")
  for r in "${roots[@]}"; do
    [ -d "$r" ] || continue
    local f
    f=$(find "$r" -maxdepth 6 -name "libd3dshared.dylib" 2>/dev/null | head -1)
    [ -n "$f" ] && { echo "$(dirname "$f")"; return 0; }
  done
  return 1
}

SRC=""
if [ $# -ge 1 ]; then
  SRC=$(find_payload "$1") || true
fi
if [ -z "$SRC" ]; then
  # Auto-detect a mounted GPTK dmg
  SRC=$(find_payload /Volumes/Game* /Volumes/*orting* 2>/dev/null) || true
fi
if [ -z "$SRC" ]; then
  # Extract from an installed CrossOver (fallback path)
  SRC=$(find_payload "/Applications/CrossOver.app/Contents/SharedSupport/CrossOver/lib64/apple_gptk") || true
fi
if [ -z "$SRC" ]; then
  echo "Could not find libd3dshared.dylib."
  echo "Check that the GPTK dmg is mounted, or pass its path as an argument."
  exit 1
fi

echo "==> GPTK payload: $SRC"
mkdir -p "$ENGINE/lib/external"
cp -Rf "$SRC/D3DMetal.framework" "$ENGINE/lib/external/" 2>/dev/null || true
cp -f  "$SRC/libd3dshared.dylib" "$ENGINE/lib/external/"
# The payload is three parts, and D3DMetal only engages with all three:
#   external/               libd3dshared.dylib + D3DMetal.framework (the Metal side)
#   wine/x86_64-windows/    d3d11.dll, d3d12.dll, dxgi.dll, atidxx64.dll, nvapi64.dll,
#                           nvngx.dll: Apple's PE shims that replace Wine's own DLLs
#                           and call into libd3dshared through the unixlib below
#   wine/x86_64-unix/       one entry per shim, a symlink to libd3dshared
# With only the first part, Wine keeps its own d3d11.dll (wined3d on Vulkan):
# slower, and the Epic Games Launcher crashes at start on it.
PE="$SRC/../wine/x86_64-windows"
if [ -d "$PE" ]; then
  for f in "$PE"/*.dll; do
    b=$(basename "$f")
    # keep Wine's own copy once, so the swap is reversible
    [ -f "$ENGINE/lib/wine/x86_64-windows/$b" ] && [ ! -f "$ENGINE/lib/wine/x86_64-windows/$b.wine" ] \
      && cp -p "$ENGINE/lib/wine/x86_64-windows/$b" "$ENGINE/lib/wine/x86_64-windows/$b.wine"
    cp -f "$f" "$ENGINE/lib/wine/x86_64-windows/$b"
  done
  SHIMS=$(cd "$PE" && ls *.dll | sed 's/\.dll$//')
else
  echo "    (no wine/x86_64-windows next to the payload: installing the Metal side only, D3D stays on wined3d)"
  SHIMS="d3d10 d3d11 d3d12 dxgi"
fi
# Wire the unixlib symlinks (never copy! @loader_path rule)
( cd "$ENGINE/lib/wine/x86_64-unix" && for f in $SHIMS; do ln -sf ../../external/libd3dshared.dylib "$f.so"; done )
echo "==> Installed to: $ENGINE/lib/external (+ $(echo $SHIMS | wc -w | tr -d ' ') PE shims in lib/wine/x86_64-windows)"
