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
# Wire the symlinks (never copy! @loader_path rule)
( cd "$ENGINE/lib/wine/x86_64-unix" && for f in d3d10.so d3d11.so d3d12.so dxgi.so; do ln -sf ../../external/libd3dshared.dylib "$f"; done )
echo "==> Installed to: $ENGINE/lib/external"
echo "    (With only libd3dshared and no D3DMetal.framework, games still run on vkd3d graphics)"
