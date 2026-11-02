#!/usr/bin/env bash
# Build the VR runtime Soju's D2R VR mode uses, from pinned sources, and pack
# it as the release file resources/d2r-vr.json points at.
#
#   wineopxdms.dll/.so   wineopenxr bridge (LGPL-2.1-or-later): the game's
#                        OpenXR calls go through Wine to the macOS runtime, and
#                        D3D12 swapchain images are D3DMetal resources backed
#                        by the runtime's Metal textures. Renamed from
#                        wineopenxr so it cannot clash with an engine's own.
#   liboxrsys-runtime    OXRSys OpenXR runtime for macOS (MPL-2.0), x86_64 so
#                        it loads inside the game's Rosetta process, plus its
#                        arm64 hardware HEVC encoder helper. It streams to a
#                        headset running the OXRSys client.
#
# Requirements: Xcode command line tools, Homebrew cmake, ninja, mingw-w64.
# Usage: scripts/build-vr-runtime.sh [output.tar.xz]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${1:-$ROOT/dist/soju-vr-runtime.tar.xz}"
WORK="${WORK:-$HOME/.battlenet-macos/build/vr-runtime}"
export PATH="/opt/homebrew/bin:$PATH"

BRIDGE_URL="https://github.com/anoshmisiosdev/oxrsys-winebridge.git"
BRIDGE_COMMIT="88a97e9"                                    # superproject
WINEOPENXR_COMMIT="1cfaac97bda3f45ac756510c59c45a630c74502a"  # bridge/ submodule
OXRSYS_COMMIT="204f0933ea5b4fe4d4f28b2248fc9727f6655d3c"      # oxrsys-src/ submodule

for tool in cmake ninja x86_64-w64-mingw32-gcc git python3 xcrun; do
  command -v "$tool" >/dev/null || { echo "Missing $tool (brew install cmake ninja mingw-w64)" >&2; exit 1; }
done

mkdir -p "$WORK"
SRC="$WORK/oxrsys-winebridge"
if [ ! -d "$SRC/.git" ]; then
  git clone --quiet "$BRIDGE_URL" "$SRC"
fi
git -C "$SRC" fetch --quiet origin
git -C "$SRC" checkout --quiet --detach "$BRIDGE_COMMIT"
git -C "$SRC" submodule update --init --recursive bridge oxrsys-src
[ "$(git -C "$SRC/bridge" rev-parse HEAD)" = "$WINEOPENXR_COMMIT" ] || { echo "bridge is not at $WINEOPENXR_COMMIT" >&2; exit 1; }
[ "$(git -C "$SRC/oxrsys-src" rev-parse HEAD)" = "$OXRSYS_COMMIT" ] || { echo "oxrsys-src is not at $OXRSYS_COMMIT" >&2; exit 1; }

echo "==> Building the wineopenxr bridge (PE + unix side, x86_64)"
cmake -S "$SRC/bridge" -B "$WORK/bridge-build" -G Ninja -DCMAKE_BUILD_TYPE=Release >/dev/null
cmake --build "$WORK/bridge-build"

echo "==> Building the OXRSys runtime (x86_64) and its encoder helper"
cmake -S "$SRC/oxrsys-src" -B "$WORK/oxrsys-build" -G Ninja -DCMAKE_OSX_ARCHITECTURES=x86_64 \
  -DCMAKE_BUILD_TYPE=Release -DOXRSYS_ENABLE_CTS=OFF >/dev/null
cmake --build "$WORK/oxrsys-build" --target oxrsys_runtime oxrsys-encoder-helper

STAGE="$WORK/stage"
rm -rf "$STAGE"; mkdir -p "$STAGE/wine/x86_64-windows" "$STAGE/wine/x86_64-unix" "$STAGE/runtime" "$STAGE/licenses"
python3 "$SRC/test/dmsubst/rename_pe_export.py" "$WORK/bridge-build/src/pe/wineopenxr.dll" \
  "$STAGE/wine/x86_64-windows/wineopxdms.dll" wineopxdms.dll >/dev/null
cp "$WORK/bridge-build/src/unix/wineopenxr.so" "$STAGE/wine/x86_64-unix/wineopxdms.so"
RT="$WORK/oxrsys-build/runtime"
cp "$RT/liboxrsys-runtime.dylib" "$RT/oxrsys-encoder-helper" "$RT/oxrsys-runtime.toml" "$STAGE/runtime/"
cat > "$STAGE/runtime/oxrsys-runtime.json" <<'JSON'
{
    "file_format_version": "1.0.0",
    "runtime": {
        "name": "OXRSys Runtime",
        "library_path": "./liboxrsys-runtime.dylib"
    }
}
JSON
for f in "$STAGE/wine/x86_64-unix/wineopxdms.so" "$STAGE/runtime/liboxrsys-runtime.dylib" "$STAGE/runtime/oxrsys-encoder-helper"; do
  codesign --force --sign - "$f"
done
cp "$SRC/bridge/LICENSE" "$STAGE/licenses/wineopenxr-LICENSE"
cp "$SRC/bridge/COPYING.LIB" "$STAGE/licenses/wineopenxr-COPYING.LIB"
cp "$SRC/oxrsys-src/LICENSE" "$STAGE/licenses/oxrsys-LICENSE"
cat > "$STAGE/SOURCES.txt" <<EOF
Built by scripts/build-vr-runtime.sh from:
  $BRIDGE_URL @ $BRIDGE_COMMIT
  wineopenxr (bridge/)  https://github.com/anoshmisiosdev/wineopenxr @ $WINEOPENXR_COMMIT  LGPL-2.1-or-later
  OXRSys (oxrsys-src/)  https://github.com/anoshmisiosdev/oxrsys @ $OXRSYS_COMMIT  MPL-2.0
EOF

mkdir -p "$(dirname "$OUT")"
COPYFILE_DISABLE=1 tar --no-mac-metadata --no-xattrs -cf - -C "$STAGE" . | xz -T0 -6 > "$OUT"
echo "==> $OUT"
shasum -a 256 "$OUT"
