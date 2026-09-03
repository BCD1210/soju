#!/usr/bin/env bash
# Install Apple's Game Porting Toolkit payload (libd3dshared, D3DMetal.framework
# and Apple's PE shims) into the engine, after verifying every file.
#
# Why it's needed: libd3dshared is required for D2R's loader (anti-cheat) to get
# through Rosetta 2 (it registers non-native code regions). It also provides the
# graphics backend (D3DMetal). Apple forbids redistributing it, so you download
# it yourself: a free Apple ID is enough.
#
# 1) Download the "Evaluation environment for Windows games" dmg (that is the
#    Game Porting Toolkit download) from
#    https://developer.apple.com/games/game-porting-toolkit/
#    (requires an Apple ID sign-in, free)
# 2) With the dmg mounted (or passing a path), run this script:
#      scripts/get-gptk.sh                  # auto-detect the mounted volume
#      scripts/get-gptk.sh /path/to/volume  # explicit path
#      scripts/get-gptk.sh --find [path]    # only print the payload directory
#
# Alternative: if CrossOver (trial included) is installed, it is extracted from
# there automatically.
#
# Verification. Everything copied here ends up loaded into every game process,
# so nothing is copied unverified:
#   - libd3dshared.dylib and D3DMetal.framework must satisfy the code
#     requirement "anchor apple" (signed by Apple itself, checked
#     cryptographically by codesign, not by reading certificate names).
#   - The PE shims (d3d11.dll and friends) are Windows binaries, which macOS
#     cannot verify, so each one must match the SHA-256 of a build Apple has
#     shipped (list below, gathered from Apple's own dmgs and from CrossOver).
#     A shim that is not on the list is not installed. A new toolkit release
#     therefore needs the list extended: run `soju update`, or open an issue
#     with the version. SOJU_GPTK_UNVERIFIED=1 installs unknown shims anyway,
#     loudly.
set -euo pipefail

ENGINE="${ENGINE:-$HOME/.battlenet-macos/cx26-engine}"

# SHA-256 of every PE shim Apple has shipped, by source. Hashes only: the file
# name does not matter, a renamed or tampered file simply fails to match.
KNOWN_SHIMS='
# Evaluation environment for Windows games 2.1 (2025-03)
3f88284a15b94840fdb674adc358855339635eaf7757d135f71d3cfdad77e8ab d3d10.dll
13a621833929d7ced7761d0cf9f9b57765345633dfb83161c2735e9cfb57b0f5 d3d11.dll
dca51fb33c5d79d36dae95131205b96c837db5c2579d0bfd5fcf3d15e887b324 d3d12.dll
5e1d256b455f744979092f901a0baeb0053ab291f53e0f6b65f5354043b56d57 dxgi.dll
18995adb10bed163b7f58ab184c747b1ae6447043292d3497a515bc32e5972b5 atidxx64.dll
# Evaluation environment for Windows games 3.0 (2025-12)
d316848cc78301829e6031b60c6f558716cdc74c440df72935968160253c03a5 d3d10.dll
05fa6f700fb5ee5561dc3a46d98f0aed845d4fa8ab274bff1dc604b0b3f6edd1 d3d11.dll
27dcf69bedb20ed557a83953e356ab2477b688c2a2c631f880b7583f30ff0705 d3d12.dll
249711d3d786a9b0bb7c3558f36e472ab07ee21b8dbf118e34c975046832f9a2 dxgi.dll
bd004dd9e41415b0e1395d51a2ded762a02c7731491e74ed6076f3648ae9ea7f atidxx64.dll
8e1e6cde85551360d7a061a09445500f3c38805651502f3be65049d937c6960c nvapi64.dll
d7c0df74d9bb4de5e2a3cc357b2309148fd3fdc824fe7941e4d789dbd072ff99 nvngx-on-metalfx.dll
# Evaluation environment for Windows games 4.0 beta 2 (2026-07)
14c84a364a1260497f0a5117ef8efd6e228764ab139a67af1127e8bd013c48c7 d3d10.dll
303b2bb41efa30c890e2e93d39c3d3c565c8557e069eee832f2cb8a37bd4ec26 d3d11.dll
1b7a02cb37ec6b484e2aaa76b5ec9cbb47e63aeec29dbe087d5d1589a3347cfb d3d12.dll
522a8b37216afb09e614489d88a74118076f4d7e08d2b289df6a6eb6f3e817af dxgi.dll
05eedf19e75c6b4c0dce918577aa6ca3fe5da79d04e42145cf66f498fad3556a nvapi64.dll
f6bc9d77fd1e898fec8c6339d367bd8e0f338992c9c0c66d59b30c6e9e0743e4 nvngx-on-metalfx.dll
# CrossOver 26.3 (lib64/apple_gptk, the build Apple supplies to CodeWeavers)
7c2bfeb66b18e3ec10c3ee92c9d42f4e3123692d568d14c831aec1a13aa03f79 d3d11.dll
bbda1c4e94ee70255c528c5689b28333ca9bece2d755ede7c4197977a534704f d3d12.dll
1b1f2d80349e043e6c628b515ba6b44478a1209c504e6c9f3dae4a9d1b06d561 dxgi.dll
c999c40698b7fc23c864165fb1364e6a40a8572469775947845afd42f4dfc9e7 atidxx64.dll
f073fc2377b305380bcd8c228394e48abe1caf09116e12875cb656774a14b4dc nvapi64.dll
'
# The shims D3DMetal cannot work without (the rest are optional extras).
REQUIRED_SHIMS="d3d11 d3d12 dxgi"

# Signed by Apple itself: codesign checks the signature and the certificate
# chain against Apple's anchor. --deep covers the framework's nested code.
verify_apple() {
  codesign --verify --deep --strict -R="anchor apple" "$1" 2>/dev/null
}
shim_known() {
  local h; h=$(shasum -a 256 "$1" | cut -d' ' -f1)
  grep -q "^$h " <<<"$KNOWN_SHIMS"
}
# The payload directory (the one holding libd3dshared.dylib) under one root,
# printed only when both Mach-O parts verify.
find_payload() {
  local r="$1" f dir
  [ -d "$r" ] || return 1
  f=$(find "$r" -maxdepth 6 -name "libd3dshared.dylib" 2>/dev/null | head -1)
  [ -n "$f" ] || return 1
  dir=$(dirname "$f")
  if ! verify_apple "$f"; then
    echo "Found $f but it is not signed by Apple, not using it" >&2; return 1
  fi
  if [ ! -d "$dir/D3DMetal.framework" ]; then
    echo "Found $f but D3DMetal.framework is not next to it, not using it" >&2; return 1
  fi
  if ! verify_apple "$dir/D3DMetal.framework"; then
    echo "Found $dir/D3DMetal.framework but it is not signed by Apple, not using it" >&2; return 1
  fi
  echo "$dir"
}
# Disk images mounted under /Volumes. Apple's dmg mounts as "Evaluation
# environment for Windows games N.N" and the name has changed between
# releases, so ask hdiutil for the mount points instead of guessing names.
mounted_images() {
  hdiutil info 2>/dev/null | awk -F'\t' '$1 ~ /^\/dev\/disk/ && $3 ~ /^\/Volumes\// {print $3}'
}
# Every candidate root: an explicit path, else mounted images, then CrossOver.
find_gptk() {
  local r
  if [ -n "${1:-}" ]; then find_payload "$1"; return; fi
  while IFS= read -r r; do
    [ -n "$r" ] && find_payload "$r" && return 0
  done <<EOF
$(mounted_images)
EOF
  find_payload "/Applications/CrossOver.app/Contents/SharedSupport/CrossOver/lib64/apple_gptk"
}

MODE=install
case "${1:-}" in
  --find) MODE=find; shift ;;
  --list-images) mounted_images; exit 0 ;;
esac
if [ "$MODE" = install ]; then
  [ -d "$ENGINE/lib" ] || { echo "Engine not found at $ENGINE. Run install.sh (prebuilt) or scripts/build-engine.sh first."; exit 1; }
fi

if ! SRC=$(find_gptk "${1:-}"); then
  [ "$MODE" = find ] && exit 1
  echo "Could not find an Apple-signed GPTK payload (libd3dshared.dylib + D3DMetal.framework)."
  echo "Disk images mounted right now:"
  mounted_images | sed 's/^/  /'
  echo "Mount the Game Porting Toolkit dmg (\"Evaluation environment for Windows games\"), or pass the path of its volume as an argument."
  exit 1
fi
if [ "$MODE" = find ]; then echo "$SRC"; exit 0; fi

echo "==> GPTK payload: $SRC"
# The payload is three parts, and D3DMetal only engages with all three:
#   external/               libd3dshared.dylib + D3DMetal.framework (the Metal side)
#   wine/x86_64-windows/    d3d11.dll, d3d12.dll, dxgi.dll, atidxx64.dll, nvapi64.dll,
#                           nvngx.dll: Apple's PE shims that replace Wine's own DLLs
#                           and call into libd3dshared through the unixlib below
#   wine/x86_64-unix/       one entry per shim, a symlink to libd3dshared
# With only the first part, Wine keeps its own d3d11.dll (wined3d on Vulkan):
# slower, and the Epic Games Launcher crashes at start on it.
PE="$SRC/../wine/x86_64-windows"
SHIMS=""
SKIPPED=""
if [ -d "$PE" ]; then
  # Verify first, copy after: a payload that fails must not leave the engine
  # half swapped (Apple's dxgi next to Wine's d3d11).
  for f in "$PE"/*.dll; do
    [ -f "$f" ] || continue
    b=$(basename "$f")
    if shim_known "$f"; then SHIMS="$SHIMS ${b%.dll}"
    elif [ "${SOJU_GPTK_UNVERIFIED:-0}" = 1 ]; then
      echo "    WARNING: $b is not a build on the known list, installing it anyway (SOJU_GPTK_UNVERIFIED=1)"
      SHIMS="$SHIMS ${b%.dll}"
    else SKIPPED="$SKIPPED $b"; fi
  done
  [ -n "$SKIPPED" ] && echo "    Not installed (no known Apple build matches their SHA-256):$SKIPPED"
  for r in $REQUIRED_SHIMS; do
    case " $SHIMS " in *" $r "*) ;; *)
      echo "ERROR: $r.dll from this payload is not a build on the known list, so D3DMetal cannot be enabled from it."
      echo "       A newer toolkit than this script knows? Run 'soju update' and retry, or open an issue with the toolkit version."
      echo "       SOJU_GPTK_UNVERIFIED=1 installs it unverified; every game loads these files, so do that only for a dmg you downloaded from Apple yourself."
      exit 1 ;;
    esac
  done
  for b in $SHIMS; do
    # keep Wine's own copy once, so the swap is reversible
    [ -f "$ENGINE/lib/wine/x86_64-windows/$b.dll" ] && [ ! -f "$ENGINE/lib/wine/x86_64-windows/$b.dll.wine" ] \
      && cp -p "$ENGINE/lib/wine/x86_64-windows/$b.dll" "$ENGINE/lib/wine/x86_64-windows/$b.dll.wine"
    cp -f "$PE/$b.dll" "$ENGINE/lib/wine/x86_64-windows/$b.dll"
  done
else
  echo "    (no wine/x86_64-windows next to the payload: installing the Metal side only, D3D stays on wined3d)"
  SHIMS="d3d10 d3d11 d3d12 dxgi"
fi
mkdir -p "$ENGINE/lib/external"
rm -rf "$ENGINE/lib/external/D3DMetal.framework"
cp -Rf "$SRC/D3DMetal.framework" "$ENGINE/lib/external/"
cp -f  "$SRC/libd3dshared.dylib" "$ENGINE/lib/external/"
# Wire the unixlib symlinks (never copy! @loader_path rule)
( cd "$ENGINE/lib/wine/x86_64-unix" && for f in $SHIMS; do ln -sf ../../external/libd3dshared.dylib "$f.so"; done )
echo "==> Installed to: $ENGINE/lib/external (+ $(echo $SHIMS | wc -w | tr -d ' ') PE shims in lib/wine/x86_64-windows)"
