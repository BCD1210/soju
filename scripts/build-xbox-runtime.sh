#!/usr/bin/env bash
# Rebuild Soju's xgameruntime.dll (the one file resources/xbox-runtime.json takes
# from Soju's own release instead of MACxus): the Xodus Nexus runtime's Wine 11.0
# tree + the two MACxus runtime patches + patches/xgameruntime-store-empty-strings.patch.
# Every input is pinned in resources/xbox-runtime.json ("soju" -> "build").
# Needs Xcode CLT, Homebrew mingw-w64 and bison. Output: dist/xbox-runtime/xgameruntime.dll
# (bytes differ between toolchains; the release pins the published build).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="${SOJU_XBOX_RUNTIME_BUILD:-$HOME/.battlenet-macos/build/xbox-runtime-src}"
OUT="${SOJU_XBOX_RUNTIME_OUT:-$ROOT/dist/xbox-runtime}"
export PATH="/opt/homebrew/opt/bison/bin:/opt/homebrew/bin:$PATH"
command -v x86_64-w64-mingw32-gcc >/dev/null || { echo "Install mingw-w64 first: brew install mingw-w64" >&2; exit 1; }

# Download $1 to $2 (kept between runs) and use it only when its SHA-256 is $3.
fetch_checked(){   # url, dest, sha256
  local got
  if [ ! -f "$2" ]; then
    curl -fL --retry 3 --proto =https "$1" -o "$2.part" || { rm -f "$2.part"; echo "Download failed: $1" >&2; return 1; }
    mv -f "$2.part" "$2"
  fi
  got=$(shasum -a 256 "$2" | awk '{print $1}')
  if [ "$got" != "$3" ]; then
    rm -f "$2"
    printf 'Checksum mismatch for %s\n  expected %s\n  got      %s\n' "$1" "$3" "$got" >&2
    return 1
  fi
}
field(){ python3 -c 'import json,sys
v=json.load(open(sys.argv[1]))["soju"]["build"]
for k in sys.argv[2].split("."): v=v[int(k)] if k.isdigit() else v[k]
print(v)' "$ROOT/resources/xbox-runtime.json" "$1"; }

mkdir -p "$WORK" "$OUT"
cd "$WORK"
fetch_checked "$(field source.url)" source.tar.xz "$(field source.sha256)"
commit="$(field macxus_commit)"
for i in 0 1; do
  path="$(field patches.$i.macxus)"
  fetch_checked "https://raw.githubusercontent.com/fladnaG86/MACxus/$commit/$path" "$(basename "$path")" "$(field patches.$i.sha256)"
done
ours="$ROOT/$(field patches.2.soju)"
[ "$(shasum -a 256 "$ours" | awk '{print $1}')" = "$(field patches.2.sha256)" ] \
  || { echo "$ours does not match resources/xbox-runtime.json" >&2; exit 1; }

# A fresh tree each run, so a half-applied patch never carries over.
rm -rf tree build64; mkdir tree
tar -xJf source.tar.xz -C tree
WINE="$WORK/tree/shared-build/wine-src"
for p in "$WORK/$(basename "$(field patches.0.macxus)")" "$WORK/$(basename "$(field patches.1.macxus)")" "$ours"; do
  patch -p1 -d "$WINE" < "$p"
done
mkdir build64 && cd build64
"$WINE/configure" --enable-archs=x86_64 --disable-tests --without-x --without-freetype > configure.log 2>&1 \
  || { tail -20 configure.log >&2; exit 1; }
make -j"${SOJU_BUILD_JOBS:-8}" dlls/xgameruntime/x86_64-windows/xgameruntime.dll > make.log 2>&1 \
  || { tail -20 make.log >&2; exit 1; }
cp dlls/xgameruntime/x86_64-windows/xgameruntime.dll "$OUT/xgameruntime.dll"
echo "Built $OUT/xgameruntime.dll"
echo "  size   $(stat -f %z "$OUT/xgameruntime.dll")"
echo "  sha256 $(shasum -a 256 "$OUT/xgameruntime.dll" | awk '{print $1}')"
