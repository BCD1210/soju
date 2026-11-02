#!/usr/bin/env bash
# Fetch the runtime components without CrossOver:
#  - x86_64 dylib stack (gnutls/nettle/MoltenVK, ...): from frankea/Whisky's GPL engine release
#  - wine-mono: from the official wine-mono release
# (D3DMetal/libd3dshared come from Apple GPTK: see get-gptk.sh)
set -euo pipefail

WORK="${WORK:-$HOME/.battlenet-macos/build}"
DEPS="$WORK/deps"; mkdir -p "$DEPS/lib" "$WORK"

# Download $1 to $2 (kept between runs) and use it only when its SHA-256 is $3.
fetch_checked(){   # url, dest, sha256
  local got
  if [ ! -f "$2" ]; then
    curl -fL --retry 3 "$1" -o "$2.part" || { rm -f "$2.part"; echo "Download failed: $1" >&2; return 1; }
    mv -f "$2.part" "$2"
  fi
  got=$(shasum -a 256 "$2" | awk '{print $1}')
  if [ "$got" != "$3" ]; then
    rm -f "$2"
    printf 'Checksum mismatch for %s\n  expected %s\n  got      %s\n' "$1" "$3" "$got" >&2
    return 1
  fi
}

echo "==> Downloading the x86_64 dylib stack (frankea/Whisky Libraries, GPL)"
LIBS_URL="https://github.com/frankea/Whisky/releases/download/v3.1.1/Libraries.tar.gz"
LIBS_SHA256="01f3a1b43b98065fe20c529c1023b61dd79a6d2ad93bba6040865f646481ccf3"
fetch_checked "$LIBS_URL" "$WORK/whisky-libs.tar.gz" "$LIBS_SHA256"
mkdir -p "$WORK/whisky-libs"
tar -xzf "$WORK/whisky-libs.tar.gz" -C "$WORK/whisky-libs"
cp -f "$WORK/whisky-libs/Libraries/Wine/lib/"*.dylib "$DEPS/lib/" 2>/dev/null
echo "    Got $(ls "$DEPS/lib"/*.dylib | wc -l | tr -d ' ') dylibs"

echo "==> Downloading wine-mono 10.4.1 (official release)"
MONO_URL="https://github.com/wine-mono/wine-mono/releases/download/wine-mono-10.4.1/wine-mono-10.4.1-x86.tar.xz"
MONO_SHA256="a16606ef0724202e6a6848ece6e0cbba64d11e2f11aefe744af1d93c6d9f99bb"
fetch_checked "$MONO_URL" "$WORK/wine-mono.tar.xz" "$MONO_SHA256"
mkdir -p "$WORK/mono"
tar -xJf "$WORK/wine-mono.tar.xz" -C "$WORK/mono"
echo "    Got mono: $(ls "$WORK/mono" | head -1)"

echo "==> Done. build-engine.sh picks this up automatically: $DEPS"
