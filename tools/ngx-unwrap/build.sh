#!/bin/sh
# Build nvngx.dll (the ReShade unwrapper for the D2R VR copy) with a MinGW-w64 x86_64 compiler:
#   tools/ngx-unwrap/build.sh [compiler] [output]
# Default compiler: x86_64-w64-mingw32-gcc (Homebrew mingw-w64) or llvm-mingw's clang.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
cc=${1:-x86_64-w64-mingw32-gcc}
out=${2:-$here/../../dist/vr-mod/soju-nvngx.dll}
mkdir -p "$(dirname "$out")"
"$cc" -O2 -Wall -shared -s -Wl,--no-insert-timestamp -o "$out" "$here/ngx_unwrap.c" "$here/nvngx.def"
shasum -a 256 "$out"; wc -c < "$out"
