#!/usr/bin/env bash
# Build the wine 11.0 engine from CrossOver 26.3's published GPL sources (free and legal).
# Verified: each step of this recipe was verified individually during the real
# 2026-08 build (Battle.net + D2R in-game). Caveat: a single clean end-to-end
# re-run is untested: issues welcome.
#
# Requirements: Apple Silicon + Rosetta 2, Xcode CLT, Homebrew (arm64).
# Run first: scripts/get-components.sh (fetches dylibs+mono: no CrossOver needed)
set -euo pipefail

WORK="${WORK:-$HOME/.battlenet-macos/build}"
REPO="$(cd "$(dirname "$0")/.." && pwd)"
SRC_URL="https://media.codeweavers.com/pub/crossover/source/crossover-sources-26.3.0.tar.gz"
FT_URL="https://download.savannah.gnu.org/releases/freetype/freetype-2.13.3.tar.gz"
DEPS="$WORK/deps"
ENGINE="${ENGINE:-$HOME/.battlenet-macos/cx26-engine}"
# get-components.sh pre-fetches the x86_64 dylibs into $DEPS/lib (frankea GPL release)

echo "==> Installing build tools (bison 3.x, mingw-w64)"
brew list bison >/dev/null 2>&1 || brew install bison
brew list mingw-w64 >/dev/null 2>&1 || brew install mingw-w64
brew list gnutls >/dev/null 2>&1 || brew install gnutls   # for headers
export PATH="/opt/homebrew/opt/bison/bin:/opt/homebrew/bin:$PATH"

mkdir -p "$WORK" "$DEPS"

echo "==> Downloading CrossOver 26.3 sources + freetype"
[ -f "$WORK/cx-src.tar.gz" ] || { curl -fL "$SRC_URL" -o "$WORK/cx-src.tar.gz.part" && mv -f "$WORK/cx-src.tar.gz.part" "$WORK/cx-src.tar.gz"; }
tar -xzf "$WORK/cx-src.tar.gz" -C "$WORK" sources/wine
WINE="$WORK/sources/wine"
# Soju winemac patch: WINE_NO_DOCK_ICON (hide helper-process Dock icons) and
# WINE_DOCK_REOPEN_CMD (Dock click brings a tray-parked launcher back). Used by
# the Epic and Steam modes of play.sh.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if ! grep -q WINE_DOCK_REOPEN_CMD "$WINE/dlls/winemac.drv/cocoa_app.m"; then
  patch -p1 -d "$WINE" < "$SCRIPT_DIR/../patches/winemac-no-dock-icon.patch"
fi
if ! grep -q "soju_append_chromium_flags" "$WINE/dlls/kernelbase/process.c"; then
  echo "==> Applying patches/chromium-flags-append.patch (SOJU_CHROMIUM_FLAGS hook for Qt launchers)"
  patch -p1 -d "$WINE" < "$SCRIPT_DIR/../patches/chromium-flags-append.patch"
fi
if ! grep -q "releasePressedKeys" "$WINE/dlls/winemac.drv/cocoa_app.m"; then
  echo "==> Applying patches/winemac-release-keys-on-focus-loss.patch (keys stuck down after a focus change)"
  patch -p1 -d "$WINE" < "$SCRIPT_DIR/../patches/winemac-release-keys-on-focus-loss.patch"
fi
if ! grep -q "KEY_STORE_MAGIC" "$WINE/dlls/ncrypt/main.c"; then
  echo "==> Applying patches/ncrypt-persisted-keys.patch (named CNG keys survive the process)"
  patch -p1 -d "$WINE" < "$SCRIPT_DIR/../patches/ncrypt-persisted-keys.patch"
fi

echo "==> Building x86_64 freetype (Rosetta)"
[ -f "$WORK/ft.tar.gz" ] || { curl -fL "$FT_URL" -o "$WORK/ft.tar.gz.part" && mv -f "$WORK/ft.tar.gz.part" "$WORK/ft.tar.gz"; }
tar -xzf "$WORK/ft.tar.gz" -C "$WORK"
( cd "$WORK/freetype-2.13.3" && arch -x86_64 ./configure --prefix="$DEPS" \
    --without-harfbuzz --without-png --without-brotli --without-bzip2 \
    --enable-shared --disable-static CC="clang -arch x86_64" \
  && arch -x86_64 make -j"$(sysctl -n hw.ncpu)" && arch -x86_64 make install )

echo "==> Checking the x86_64 dependency dylibs (get-components.sh output)"
ls "$DEPS/lib/libgnutls.30.dylib" "$DEPS/lib/libMoltenVK.dylib" >/dev/null 2>&1 \
  || { echo "dylibs missing, run scripts/get-components.sh first"; exit 1; }

echo "==> wine configure (x86_64, new-wow64 i386+x86_64)"
mkdir -p "$WORK/wine-build"; cd "$WORK/wine-build"
arch -x86_64 "$WINE/configure" \
  --enable-archs=i386,x86_64 --without-x --disable-tests \
  FREETYPE_CFLAGS="-I$DEPS/include/freetype2" FREETYPE_LIBS="-L$DEPS/lib -lfreetype" \
  GNUTLS_CFLAGS="-I/opt/homebrew/include" GNUTLS_LIBS="-L$DEPS/lib -lgnutls" \
  INOTIFY_CFLAGS="-I$REPO/third_party/libinotify-kqueue" INOTIFY_LIBS="-L$DEPS/lib -linotify" \
  LDFLAGS="-L$DEPS/lib" CC="clang -arch x86_64" CXX="clang++ -arch x86_64"
# wineserver must link libinotify (kqueue-backed inotify shim from the dylib stack):
# Wine implements ReadDirectoryChangesW on top of inotify only. Without it the
# Epic Games Launcher never sees its install helper's reply (DP-05 / DP-06).
grep -q '#define HAVE_SYS_INOTIFY_H 1' include/config.h \
  || { echo "configure did not pick up sys/inotify.h; check third_party/libinotify-kqueue"; exit 1; }
# Fix the soname to the dylib's real name
sed -i '' 's|#define SONAME_LIBGNUTLS.*|#define SONAME_LIBGNUTLS "libgnutls.30.dylib"|' include/config.h

echo "==> wine make"
arch -x86_64 make -j"$(sysctl -n hw.ncpu)"
arch -x86_64 make install DESTDIR="$WORK/install"

echo "==> Assembling the engine (dylibs + D3DMetal + rpath + entitlement signing)"
rm -rf "$ENGINE"; mkdir -p "$ENGINE"
cp -Rc "$WORK/install/usr/local/." "$ENGINE/"
cp -c "$DEPS/lib/"*.dylib "$ENGINE/lib/" 2>/dev/null || true
# D3DMetal/libd3dshared (Apple GPTK) is a separate step: run scripts/get-gptk.sh
# (libd3dshared is required for the game loader to pass Rosetta; without it,
# graphics fall back to vkd3d)
# wine-mono 10.4.1 (official release fetched by get-components.sh)
mkdir -p "$ENGINE/share/wine/mono"
cp -Rc "$WORK/mono/wine-mono-10.4.1" "$ENGINE/share/wine/mono/" 2>/dev/null \
  || echo "WARNING: mono missing, check that get-components.sh was run"
# rpath + entitlement signing (Rosetta / executable memory)
cat > "$WORK/ent.plist" <<'PL'
<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>com.apple.security.cs.allow-unsigned-executable-memory</key><true/>
<key>com.apple.security.cs.disable-executable-page-protection</key><true/>
<key>com.apple.security.cs.disable-library-validation</key><true/>
<key>com.apple.security.cs.allow-jit</key><true/>
</dict></plist>
PL
for b in wine wine64 wineserver wine-preloader; do
  f="$ENGINE/bin/$b"; [ -f "$f" ] || continue
  install_name_tool -add_rpath "@loader_path/../lib" "$f" 2>/dev/null || true
  # the dylib stack's libinotify carries a bare install name; point it at rpath
  install_name_tool -change libinotify.dylib @rpath/libinotify.0.dylib "$f" 2>/dev/null || true
  codesign -f -s - --entitlements "$WORK/ent.plist" "$f" 2>/dev/null || true
done

echo "==> Done: $ENGINE"
DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib" "$ENGINE/bin/wine" --version
echo
echo "Example launch:"
echo "  export WINEPREFIX=<bottle> CX_GRAPHICS_BACKEND=d3dmetal WINEMSYNC=1 WINE_SIMULATE_WRITECOPY=1"
echo "  export DYLD_FALLBACK_LIBRARY_PATH='$ENGINE/lib:/usr/lib'"
echo "  '$ENGINE/bin/wine' 'C:\\\\Program Files (x86)\\\\Battle.net\\\\Battle.net Launcher.exe' --disable-gpu-compositing"
echo "  (Apple-protected binaries (nohup, ...) in the chain strip DYLD_*, background it with a subshell & instead)"
