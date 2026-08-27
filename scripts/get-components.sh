#!/usr/bin/env bash
# CrossOver 없이 런타임 구성요소를 확보한다:
#  - x86_64 dylib 스택(gnutls/nettle/MoltenVK 등): frankea/Whisky의 GPL 엔진 릴리스에서
#  - wine-mono: 공식 wine-mono 릴리스에서
# (D3DMetal/libd3dshared는 Apple GPTK — get-gptk.sh 참고)
set -euo pipefail

WORK="${WORK:-$HOME/.battlenet-macos/build}"
DEPS="$WORK/deps"; mkdir -p "$DEPS/lib" "$WORK"

echo "==> x86_64 dylib 스택 다운로드 (frankea/Whisky Libraries, GPL)"
LIBS_URL="https://github.com/frankea/Whisky/releases/download/v3.1.1/Libraries.tar.gz"
[ -f "$WORK/whisky-libs.tar.gz" ] || curl -fL "$LIBS_URL" -o "$WORK/whisky-libs.tar.gz"
mkdir -p "$WORK/whisky-libs"
tar -xzf "$WORK/whisky-libs.tar.gz" -C "$WORK/whisky-libs"
cp -f "$WORK/whisky-libs/Libraries/Wine/lib/"*.dylib "$DEPS/lib/" 2>/dev/null
echo "    dylib $(ls "$DEPS/lib"/*.dylib | wc -l | tr -d ' ')개 확보"

echo "==> wine-mono 10.4.1 다운로드 (공식 릴리스)"
MONO_URL="https://github.com/wine-mono/wine-mono/releases/download/wine-mono-10.4.1/wine-mono-10.4.1-x86.tar.xz"
[ -f "$WORK/wine-mono.tar.xz" ] || curl -fL "$MONO_URL" -o "$WORK/wine-mono.tar.xz"
mkdir -p "$WORK/mono"
tar -xJf "$WORK/wine-mono.tar.xz" -C "$WORK/mono"
echo "    mono 확보: $(ls "$WORK/mono" | head -1)"

echo "==> 완료. build-engine.sh가 이 경로를 자동 사용합니다: $DEPS"
