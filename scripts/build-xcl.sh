#!/bin/bash
# build-xcl.sh <MacBox checkout>: build the XCL bundle `soju xcl setup` downloads.
#
# XCL (github.com/xcl-dev/xcl, GPL-2.0-only) runs Xbox Series X|S game code on Windows;
# MacBox (github.com/aykhanium/MacBox) is its Apple Silicon port, built here unchanged at the
# commit pinned in resources/xcl.json, with its own scripts/macos.sh (rustup, LLVM-MinGW and
# DXC, each pinned by SHA-256 there). The bundle holds xcl.exe, LLVM-MinGW's libunwind.dll,
# XCL's license and a NOTICE naming the exact source; Microsoft's DXC is not redistributed:
# setup downloads it from Microsoft's release (pinned in resources/xcl.json).
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
SRC=$(cd "${1:?MacBox checkout}" && pwd)
COMMIT=$(python3 -c "import json;print(json.load(open('$ROOT/resources/xcl.json'))['bundle']['commit'])")
[ "$(git -C "$SRC" rev-parse HEAD)" = "$COMMIT" ] || { echo "MacBox must be at $COMMIT" >&2; exit 1; }
git -C "$SRC" diff --quiet HEAD -- || { echo "MacBox checkout has local changes" >&2; exit 1; }
(cd "$SRC" && bash scripts/macos.sh bootstrap && bash scripts/macos.sh build)
OUT="$SRC/target/x86_64-pc-windows-gnullvm/release"
STAGE=$(mktemp -d)/soju-xcl
mkdir -p "$STAGE"
cp "$OUT/xcl.exe" "$OUT/libunwind.dll" "$STAGE/"
cp "$SRC/LICENSE" "$STAGE/LICENSE"
cat > "$STAGE/NOTICE" <<NOTE
xcl.exe: XCL (https://github.com/xcl-dev/xcl) as ported to Apple Silicon by MacBox,
built unchanged from https://github.com/aykhanium/MacBox/tree/$COMMIT
(GPL-2.0-only, see LICENSE; the source is at that address).
libunwind.dll: LLVM-MinGW 20261117 (Apache-2.0 WITH LLVM-exception).
NOTE
mkdir -p "$ROOT/dist/xcl"
tar -C "$(dirname "$STAGE")" -cJf "$ROOT/dist/xcl/soju-xcl-${COMMIT:0:7}.tar.xz" soju-xcl
ls -l "$ROOT/dist/xcl/soju-xcl-${COMMIT:0:7}.tar.xz"
shasum -a 256 "$ROOT/dist/xcl/soju-xcl-${COMMIT:0:7}.tar.xz"
