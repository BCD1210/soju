#!/usr/bin/env bash
# Fetch the runtime components without CrossOver:
#  - x86_64 dylib stack (gnutls/nettle/MoltenVK, ...): from frankea/Whisky's GPL engine release
#  - wine-mono: from the official wine-mono release
# (D3DMetal/libd3dshared come from Apple GPTK: see get-gptk.sh)
set -euo pipefail

WORK="${WORK:-$HOME/.battlenet-macos/build}"
DEPS="$WORK/deps"; mkdir -p "$DEPS/lib" "$WORK"

echo "==> Downloading the x86_64 dylib stack (frankea/Whisky Libraries, GPL)"
LIBS_URL="https://github.com/frankea/Whisky/releases/download/v3.1.1/Libraries.tar.gz"
[ -f "$WORK/whisky-libs.tar.gz" ] || { curl -fL "$LIBS_URL" -o "$WORK/whisky-libs.tar.gz.part" && mv -f "$WORK/whisky-libs.tar.gz.part" "$WORK/whisky-libs.tar.gz"; }
mkdir -p "$WORK/whisky-libs"
tar -xzf "$WORK/whisky-libs.tar.gz" -C "$WORK/whisky-libs"
cp -f "$WORK/whisky-libs/Libraries/Wine/lib/"*.dylib "$DEPS/lib/" 2>/dev/null
echo "    Got $(ls "$DEPS/lib"/*.dylib | wc -l | tr -d ' ') dylibs"

echo "==> Downloading wine-mono 10.4.1 (official release)"
MONO_URL="https://github.com/wine-mono/wine-mono/releases/download/wine-mono-10.4.1/wine-mono-10.4.1-x86.tar.xz"
[ -f "$WORK/wine-mono.tar.xz" ] || { curl -fL "$MONO_URL" -o "$WORK/wine-mono.tar.xz.part" && mv -f "$WORK/wine-mono.tar.xz.part" "$WORK/wine-mono.tar.xz"; }
mkdir -p "$WORK/mono"
tar -xJf "$WORK/wine-mono.tar.xz" -C "$WORK/mono"
echo "    Got mono: $(ls "$WORK/mono" | head -1)"

echo "==> Done. build-engine.sh picks this up automatically: $DEPS"
