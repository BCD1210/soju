#!/usr/bin/env bash
# Build the launcher-support component: what turns a copy of the CX engine into
# the engine the EA app, the Rockstar Games Launcher and Ubisoft Connect run on
# (scripts/launcher-engine.sh). Nothing installed is touched.
#
#   winemac.so   the engine's Mac driver (scripts/build-engine.sh, with
#                patches/engine/0007-remote-child-layers.patch): these launchers draw
#                their browser UI from a GPU process into a window another process
#                owns, which the driver now shows in the owner's window.
#   dxmt/        DXMT (d3d10core, d3d11, dxgi, winemetal) built from 3Shain/dxmt
#                with patches/dxmt-cross-process-window.patch, so the GPU process may
#                create a swapchain on that window.
#
# Needs the engine build tree from scripts/build-engine.sh ($WORK/sources/wine and
# $WORK/wine-build). Prints the manifest for resources/launcher-support.json.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="${WORK:-$HOME/.battlenet-macos/build}"
WINE="$WORK/sources/wine"
VERSION="${1:-launcher-support-v1}"
OUT="${OUT:-$WORK/$VERSION}"
# DXMT master after v0.80: v0.80 lacks ID3D11DeviceContext1::SwapDeviceContextState, which the
# Rockstar Games Launcher calls at start.
DXMT_COMMIT="0df86108348bab2aaa5c4610a5eb63a9de9047ac"
# DXMT's toolchains (x86_64 LLVM 15 and the v8.16-3shain Wine tools), as scripts/build-steam-support.sh sets them up.
DXMT_TOOLCHAINS="${DXMT_TOOLCHAINS:-${SOJU_STEAM_BUILD:-$HOME/.battlenet-macos/steam-build}/dxmt/toolchains}"
MESON="${MESON:-${SOJU_STEAM_BUILD:-$HOME/.battlenet-macos/steam-build}/venv/bin/meson}"
LLVM_LICENSE_URL="https://raw.githubusercontent.com/llvm/llvm-project/llvmorg-15.0.7/llvm/LICENSE.TXT"
export PATH="/opt/homebrew/opt/bison/bin:/opt/homebrew/bin:$PATH"

[ -d "$WINE" ] && [ -f "$WORK/wine-build/Makefile" ] \
  || { echo "Engine build tree not found in $WORK; run scripts/build-engine.sh first" >&2; exit 1; }

fetch_checked(){   # url, dest, sha256
  local got
  [ -f "$2" ] || { curl -fL --retry 3 "$1" -o "$2.part" && mv -f "$2.part" "$2"; }
  got=$(shasum -a 256 "$2" | awk '{print $1}')
  [ "$got" = "$3" ] || { rm -f "$2"; echo "Checksum mismatch for $1 (got $got)" >&2; return 1; }
}

grep -q WM_MACDRV_PLACE_REMOTE_LAYER "$WINE/dlls/winemac.drv/macdrv.h" \
  || { echo "The engine tree lacks patches/engine/0007-remote-child-layers.patch; rerun scripts/build-engine.sh" >&2; exit 1; }
echo "==> Building winemac.so"
( cd "$WORK/wine-build" && arch -x86_64 make -j"$(sysctl -n hw.ncpu)" dlls/winemac.drv/winemac.so )

echo "==> DXMT $DXMT_COMMIT"
[ -x "$MESON" ] && [ -d "$DXMT_TOOLCHAINS/llvm" ] && [ -d "$DXMT_TOOLCHAINS/wine" ] \
  || { echo "DXMT toolchains not found ($DXMT_TOOLCHAINS); run scripts/build-steam-support.sh first" >&2; exit 1; }
DX="$WORK/dxmt-${DXMT_COMMIT:0:10}"
if [ ! -d "$DX/.git" ]; then
  git init -q "$DX" && git -C "$DX" remote add origin https://github.com/3Shain/dxmt.git
  git -C "$DX" fetch -q --depth 1 origin "$DXMT_COMMIT" && git -C "$DX" checkout -q FETCH_HEAD
  git -C "$DX" submodule update --init --depth 1 include/native/directx
fi
[ "$(git -C "$DX" rev-parse HEAD)" = "$DXMT_COMMIT" ] || { echo "DXMT checkout is not $DXMT_COMMIT" >&2; exit 1; }
git -C "$DX" apply --reverse --check "$ROOT/patches/dxmt-cross-process-window.patch" 2>/dev/null \
  || git -C "$DX" apply "$ROOT/patches/dxmt-cross-process-window.patch"
mkdir -p "$DX/toolchains"; ln -sfn "$DXMT_TOOLCHAINS/llvm" "$DX/toolchains/llvm"; ln -sfn "$DXMT_TOOLCHAINS/wine" "$DX/toolchains/wine"
( cd "$DX" && export MACOSX_DEPLOYMENT_TARGET=14.0 \
  && { [ -f build64/build.ninja ] || "$MESON" setup --cross-file build-win64.txt -Dnative_llvm_path=toolchains/llvm \
         -Dwine_install_path=toolchains/wine build64 --buildtype release --prefix "$DX/install64" --strip; } \
  && "$MESON" compile -C build64 && "$MESON" install -C build64 \
  && { [ -f build32/build.ninja ] || "$MESON" setup --cross-file build-win32.txt -Dwine_install_path=toolchains/wine \
         build32 --buildtype release --prefix "$DX/install32" --strip; } \
  && "$MESON" compile -C build32 && "$MESON" install -C build32 )
rm -rf "$OUT"; mkdir -p "$OUT/dxmt/x86_64-windows" "$OUT/dxmt/i386-windows" "$OUT/dxmt/x86_64-unix" "$OUT/licenses"
for f in d3d10core d3d11 dxgi winemetal; do
  cp "$(find "$DX/install64" -name "$f.dll" | head -1)" "$OUT/dxmt/x86_64-windows/"
  cp "$(find "$DX/install32" -name "$f.dll" | head -1)" "$OUT/dxmt/i386-windows/"
done
cp "$(find "$DX/install64" -name winemetal.so | head -1)" "$OUT/dxmt/x86_64-unix/"
cp "$WORK/wine-build/dlls/winemac.drv/winemac.so" "$OUT/winemac.so"

cp "$WINE/COPYING.LIB" "$OUT/licenses/Wine-LGPL.txt"
cp "$DX/LICENSE" "$OUT/licenses/DXMT.txt"
curl -fsSL "$LLVM_LICENSE_URL" -o "$OUT/licenses/LLVM.txt"
cat > "$OUT/SOURCES.txt" <<EOF
winemac.so: Wine from CodeWeavers' crossover-sources-26.3.0.tar.gz with the patches
  scripts/build-engine.sh applies (https://github.com/BCD1210/soju). Built by
  scripts/build-launcher-support.sh.
dxmt/: DXMT, https://github.com/3Shain/dxmt at $DXMT_COMMIT, with
  patches/dxmt-cross-process-window.patch. LLVM 15.0.7 is linked into winemetal.so.
EOF

echo "==> Packing"
ARCHIVE="$WORK/soju-$VERSION.tar.gz"
( cd "$OUT" && COPYFILE_DISABLE=1 tar --uid 0 --gid 0 -czf "$ARCHIVE" \
    SOURCES.txt winemac.so licenses dxmt )
python3 - "$OUT" "$ARCHIVE" "$VERSION" <<'PY'
import hashlib, json, pathlib, sys
out, archive, version = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3]
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
files = {str(p.relative_to(out)): sha(p) for p in sorted(out.rglob("*")) if p.is_file()}
print(json.dumps({"version": version,
                  "url": "https://github.com/BCD1210/soju/releases/download/%s/%s" % (version, archive.name),
                  "sha256": sha(archive), "files": files}, indent=2))
PY
