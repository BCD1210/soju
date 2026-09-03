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

# The payload directory (the one holding libd3dshared.dylib) under one root.
# The file is copied into the engine and loaded into every game, so only a
# copy carrying Apple's signature is accepted.
find_payload() {
  local r="$1" f
  [ -d "$r" ] || return 1
  f=$(find "$r" -maxdepth 6 -name "libd3dshared.dylib" 2>/dev/null | head -1)
  [ -n "$f" ] || return 1
  local auth; auth=$(codesign -dvv "$f" 2>&1 || true)
  case "$auth" in *"Authority=Apple Root CA"*) ;; *) auth="" ;; esac
  if [ -z "$auth" ] || ! codesign -v "$f" 2>/dev/null; then
    echo "Found $f but it does not carry Apple's signature, not using it" >&2
    return 1
  fi
  dirname "$f"
}
# Disk images mounted under /Volumes. Apple's dmg mounts as "Evaluation
# environment for Windows games N.N" and the name has changed between
# releases, so ask hdiutil for the mount points instead of guessing names.
mounted_images() {
  hdiutil info 2>/dev/null | awk -F'\t' '$1 ~ /^\/dev\/disk/ && $3 ~ /^\/Volumes\// {print $3}'
}

SRC=""
if [ $# -ge 1 ]; then
  SRC=$(find_payload "$1") || true
fi
if [ -z "$SRC" ]; then
  while IFS= read -r r; do
    [ -n "$r" ] && SRC=$(find_payload "$r") && break
  done <<EOF
$(mounted_images)
EOF
fi
if [ -z "$SRC" ]; then
  # Extract from an installed CrossOver (fallback path)
  SRC=$(find_payload "/Applications/CrossOver.app/Contents/SharedSupport/CrossOver/lib64/apple_gptk") || true
fi
if [ -z "$SRC" ]; then
  echo "Could not find libd3dshared.dylib."
  echo "Disk images mounted right now:"
  mounted_images | sed 's/^/  /'
  echo "Mount the Game Porting Toolkit dmg (\"Evaluation environment for Windows games\"), or pass the path of its volume as an argument."
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
