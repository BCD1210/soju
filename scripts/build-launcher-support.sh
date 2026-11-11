#!/usr/bin/env bash
# Build the launcher-support component: what turns a copy of the CX engine into
# the engine the EA app, the Rockstar Games Launcher and Ubisoft Connect run on
# (scripts/launcher-engine.sh). Nothing installed is touched.
#
#   winemac.so   the engine's Mac driver with patches/winemac-cross-process-swapchains.patch:
#                these launchers draw their browser UI from a GPU process into a
#                window another process owns (Wine bug 60263), which the stock
#                driver cannot present, so their windows stay white or black.
#   dxmt/        DXMT (d3d10core, d3d11, dxgi, winemetal) from Highball's branch,
#                which presents across processes when DXMT_ALLOW_CROSS_PROCESS_SWAPCHAIN=1.
#
# Needs the engine build tree from scripts/build-engine.sh ($WORK/sources/wine and
# $WORK/wine-build). Prints the manifest for resources/launcher-support.json.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="${WORK:-$HOME/.battlenet-macos/build}"
WINE="$WORK/sources/wine"
VERSION="${1:-launcher-support-v1}"
OUT="${OUT:-$WORK/$VERSION}"
DXMT_URL="https://github.com/gauthierpiarrette/highball-engine/releases/download/dxmt-highball-20260930T161516Z-a444716/dxmt-highball-20260930T161516Z-a444716.tar.gz"
DXMT_SHA256="96feaf5d98d3c6cdc3edea9503e2891d435df8d5bfdf7d3f6998a56ec62bbd58"
DXMT_COMMIT="a44471614f7cf4817e4eee717993eb19e35184b9"
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

if ! grep -q WM_MACDRV_CREATE_REMOTE_LAYER "$WINE/dlls/winemac.drv/macdrv.h"; then
  echo "==> Applying patches/winemac-cross-process-swapchains.patch"
  patch -p1 -d "$WINE" < "$ROOT/patches/winemac-cross-process-swapchains.patch"
fi
echo "==> Building winemac.so"
( cd "$WORK/wine-build" && arch -x86_64 make -j"$(sysctl -n hw.ncpu)" dlls/winemac.drv/winemac.so )

echo "==> DXMT (Highball branch, $DXMT_COMMIT)"
fetch_checked "$DXMT_URL" "$WORK/dxmt-highball.tar.gz" "$DXMT_SHA256"
rm -rf "$OUT" "$WORK/dxmt-highball"; mkdir -p "$OUT/dxmt" "$OUT/licenses" "$WORK/dxmt-highball"
tar -xzf "$WORK/dxmt-highball.tar.gz" -C "$WORK/dxmt-highball"
for arch in x86_64-windows i386-windows; do
  mkdir -p "$OUT/dxmt/$arch"
  for f in d3d10core d3d11 dxgi winemetal; do cp "$WORK/dxmt-highball/highball/$arch/$f.dll" "$OUT/dxmt/$arch/"; done
done
mkdir -p "$OUT/dxmt/x86_64-unix"
cp "$WORK/dxmt-highball/highball/x86_64-unix/winemetal.so" "$OUT/dxmt/x86_64-unix/"
cp "$WORK/wine-build/dlls/winemac.drv/winemac.so" "$OUT/winemac.so"

cp "$WINE/COPYING.LIB" "$OUT/licenses/Wine-LGPL.txt"
curl -fsSL "https://raw.githubusercontent.com/gauthierpiarrette/dxmt/$DXMT_COMMIT/LICENSE" -o "$OUT/licenses/DXMT.txt"
curl -fsSL "$LLVM_LICENSE_URL" -o "$OUT/licenses/LLVM.txt"
cat > "$OUT/SOURCES.txt" <<EOF
winemac.so: Wine from CodeWeavers' crossover-sources-26.3.0.tar.gz with the patches
  scripts/build-engine.sh applies, plus patches/winemac-cross-process-swapchains.patch
  (https://github.com/BCD1210/soju). Built by scripts/build-launcher-support.sh.
dxmt/: DXMT, https://github.com/gauthierpiarrette/dxmt at $DXMT_COMMIT
  (prebuilt: $DXMT_URL, sha256 $DXMT_SHA256). LLVM is linked into winemetal.so.
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
