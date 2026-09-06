#!/usr/bin/env bash
# Build redistribution-safe Steam components without touching installed Wine or bottles.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUILD="${SOJU_STEAM_BUILD:-$HOME/.battlenet-macos/steam-build}"
export PATH="/opt/homebrew/opt/bison/bin:/opt/homebrew/opt/flex/bin:/opt/homebrew/bin:$PATH"
JOBS="${SOJU_BUILD_JOBS:-8}"
mkdir -p "$BUILD"
cd "$BUILD"
clone(){
  if [ "${SOJU_SOURCE_ARCHIVE:-0}" = 1 ]; then [ -d "$2" ] || { echo "Extract $2 from the source release first"; exit 1; }
  else [ -d "$2/.git" ] || git clone --depth 1 --branch "$3" "$1" "$2"; fi
}
clone https://github.com/notpop/dxmt.git dxmt debug/present-path-tracing
if [ "${SOJU_SOURCE_ARCHIVE:-0}" != 1 ]; then
  git -C dxmt checkout --detach 924a607e3eee06fad5be6f176d8510bb08bc418d
  git -C dxmt submodule update --init --recursive
fi
clone https://github.com/llvm/llvm-project.git llvm-project llvmorg-15.0.7
clone https://github.com/wine-mirror/wine.git wine wine-11.0
apply_once(){
  git -C "$1" apply --reverse --check "$2" 2>/dev/null || git -C "$1" apply "$2"
}
apply_once wine "$ROOT/patches/steam-winemac.patch"
apply_once dxmt "$ROOT/patches/steam-dxmt-build.patch"
export MACOSX_DEPLOYMENT_TARGET=26.0
[ -x venv/bin/meson ] || { python3 -m venv venv; venv/bin/pip install 'meson==1.10.2'; }
mkdir -p dxmt/toolchains/wine
if [ ! -x dxmt/toolchains/wine/bin/winebuild ]; then
  curl -fL --retry 3 https://github.com/3Shain/wine/releases/download/v8.16-3shain/wine.tar.gz -o wine-toolchain.tar.gz
  tar -xzf wine-toolchain.tar.gz -C dxmt/toolchains/wine
fi
if [ ! -f dxmt/toolchains/llvm/lib/libLLVMCore.a ]; then
  cmake -G Ninja -S llvm-project/llvm -B llvm-build \
    -DCMAKE_POLICY_VERSION_MINIMUM=3.5 -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_OSX_ARCHITECTURES=x86_64 -DCMAKE_OSX_DEPLOYMENT_TARGET=14.0 \
    -DLLVM_ENABLE_PROJECTS='' -DLLVM_TARGETS_TO_BUILD='X86;AArch64' \
    -DLLVM_BUILD_TOOLS=OFF -DLLVM_BUILD_EXAMPLES=OFF -DLLVM_INCLUDE_TESTS=OFF \
    -DLLVM_INCLUDE_EXAMPLES=OFF -DLLVM_ENABLE_RTTI=ON \
    -DLLVM_ENABLE_ZSTD=OFF -DLLVM_ENABLE_ZLIB=OFF -DLLVM_ENABLE_LIBXML2=OFF \
    -DCMAKE_INSTALL_PREFIX="$BUILD/dxmt/toolchains/llvm"
  cmake --build llvm-build --parallel "$JOBS"
  cmake --install llvm-build
fi
cd dxmt
[ -f build/build.ninja ] || ../venv/bin/meson setup build --cross-file build-win64.txt \
  -Dnative_llvm_path=toolchains/llvm -Dwine_install_path=toolchains/wine --buildtype release
../venv/bin/meson compile -C build -j "$JOBS"
cd ../wine
mkdir -p build
cd build
[ -f Makefile ] || arch -x86_64 ../configure --enable-win64 --disable-tests --without-freetype \
  CFLAGS='-fvisibility=default -O2 -Wno-error -mmacosx-version-min=14.0' \
  CXXFLAGS='-fvisibility=default -O2 -Wno-error -mmacosx-version-min=14.0'
arch -x86_64 make -j"$JOBS" dlls/winemac.drv/winemac.so
cd "$BUILD"
mkdir -p output/dxmt-x64 output/licenses
for dll in d3d11 dxgi d3d10core winemetal; do
  f=$(find dxmt/build -name "$dll.dll" -type f | head -1)
  [ -n "$f" ]; cp "$f" "output/dxmt-x64/$dll.dll"
done
cp dxmt/build/src/winemetal/unix/winemetal.so output/dxmt-x64/
cp wine/build/dlls/winemac.drv/winemac.so output/winemac-patched.so
x86_64-w64-mingw32-gcc -O2 -Wall -municode -DUNICODE -D_UNICODE \
  -o output/steamwebhelper-wrapper.exe "$ROOT/third_party/steam-on-m1-wine/steamwebhelper-wrapper.c" -static -lshell32 -mwindows
cp "$ROOT/third_party/steam-on-m1-wine/LICENSE" output/licenses/wrapper-MIT.txt
cp wine/COPYING.LIB output/licenses/Wine-LGPL.txt
cp dxmt/LICENSE output/licenses/DXMT-MIT.txt
cp llvm-project/llvm/LICENSE.TXT output/licenses/LLVM.txt
cp dxmt/include/native/directx/COPYING.MinGW-w64.txt output/licenses/DirectX-headers.txt
if [ "${SOJU_SOURCE_ARCHIVE:-0}" = 1 ]; then
  cp SOURCE-REVISIONS.txt output/SOURCES.txt
else
  {
    printf 'DXMT: '; git -C dxmt rev-parse HEAD
    printf 'Wine: '; git -C wine rev-parse HEAD
    printf 'LLVM: '; git -C llvm-project rev-parse HEAD
    printf 'Soju scripts and patches: '; git -C "$ROOT" rev-parse HEAD
  } > output/SOURCES.txt
fi
printf 'Artifacts: %s/output\n' "$BUILD"
