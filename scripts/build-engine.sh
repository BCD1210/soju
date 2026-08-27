#!/usr/bin/env bash
# CrossOver 26.3의 GPL 공개 소스에서 wine 11.0 엔진을 직접 빌드한다 (무료·합법).
# 검증: 이 레시피의 각 단계는 2026-08 실제 빌드에서 개별 검증됨 (Battle.net + D2R 인게임 구동 확인).
# 주의: 처음부터 끝까지 한 번에 재실행은 미검증 — 이슈 환영.
#
# 요구: Apple Silicon + Rosetta 2, Xcode CLT, Homebrew(arm64).
set -euo pipefail

WORK="${WORK:-$HOME/.battlenet-macos/build}"
SRC_URL="https://media.codeweavers.com/pub/crossover/source/crossover-sources-26.3.0.tar.gz"
FT_URL="https://download.savannah.gnu.org/releases/freetype/freetype-2.13.3.tar.gz"
DEPS="$WORK/deps"
ENGINE="${ENGINE:-$HOME/.battlenet-macos/cx26-engine}"
# x86_64 dylib 공급처(gnutls 스택/MoltenVK): 설치된 CrossOver 또는 Whisky 엔진
DYLIB_SRC="${DYLIB_SRC:-/Applications/CrossOver.app/Contents/SharedSupport/CrossOver/lib64}"
CXG="/Applications/CrossOver.app/Contents/SharedSupport/CrossOver/lib64/apple_gptk/external"

echo "==> 빌드 도구 설치 (bison 3.x, mingw-w64)"
brew list bison >/dev/null 2>&1 || brew install bison
brew list mingw-w64 >/dev/null 2>&1 || brew install mingw-w64
brew list gnutls >/dev/null 2>&1 || brew install gnutls   # 헤더용
export PATH="/opt/homebrew/opt/bison/bin:/opt/homebrew/bin:$PATH"

mkdir -p "$WORK" "$DEPS"

echo "==> CrossOver 26.3 소스 + freetype 다운로드"
[ -f "$WORK/cx-src.tar.gz" ] || curl -fL "$SRC_URL" -o "$WORK/cx-src.tar.gz"
tar -xzf "$WORK/cx-src.tar.gz" -C "$WORK" sources/wine
WINE="$WORK/sources/wine"

echo "==> x86_64 freetype 빌드 (Rosetta)"
[ -f "$WORK/ft.tar.gz" ] || curl -fL "$FT_URL" -o "$WORK/ft.tar.gz"
tar -xzf "$WORK/ft.tar.gz" -C "$WORK"
( cd "$WORK/freetype-2.13.3" && arch -x86_64 ./configure --prefix="$DEPS" \
    --without-harfbuzz --without-png --without-brotli --without-bzip2 \
    --enable-shared --disable-static CC="clang -arch x86_64" \
  && arch -x86_64 make -j"$(sysctl -n hw.ncpu)" && arch -x86_64 make install )

echo "==> x86_64 의존 dylib 확보 (gnutls 스택, MoltenVK)"
# gnutls/nettle/gmp/... + libMoltenVK (CrossOver lib64 또는 Whisky 엔진 lib에서 조달)
for d in libgnutls.30 libnettle.8 libhogweed.6 libgmp.10 libidn2.0 libunistring.5 \
         libtasn1.6 libp11-kit.0 libintl.8 libiconv.2 libMoltenVK; do
  find "$DYLIB_SRC" -name "$d*.dylib" -exec cp -c {} "$DEPS/lib/" \; 2>/dev/null || true
done

echo "==> wine configure (x86_64, new-wow64 i386+x86_64)"
mkdir -p "$WORK/wine-build"; cd "$WORK/wine-build"
arch -x86_64 "$WINE/configure" \
  --enable-archs=i386,x86_64 --without-x --disable-tests \
  FREETYPE_CFLAGS="-I$DEPS/include/freetype2" FREETYPE_LIBS="-L$DEPS/lib -lfreetype" \
  GNUTLS_CFLAGS="-I/opt/homebrew/include" GNUTLS_LIBS="-L$DEPS/lib -lgnutls" \
  LDFLAGS="-L$DEPS/lib" CC="clang -arch x86_64" CXX="clang++ -arch x86_64"
# soname 보정 (dylib 실제 이름으로)
sed -i '' 's|#define SONAME_LIBGNUTLS.*|#define SONAME_LIBGNUTLS "libgnutls.30.dylib"|' include/config.h

echo "==> wine make"
arch -x86_64 make -j"$(sysctl -n hw.ncpu)"
arch -x86_64 make install DESTDIR="$WORK/install"

echo "==> 엔진 조립 (dylib 배치 + D3DMetal + rpath + entitlement 서명)"
rm -rf "$ENGINE"; mkdir -p "$ENGINE"
cp -Rc "$WORK/install/usr/local/." "$ENGINE/"
cp -c "$DEPS/lib/"*.dylib "$ENGINE/lib/" 2>/dev/null || true
# D3DMetal(Apple GPTK, 재배포 불가 → 설치된 CrossOver에서 조달) — 그래픽용
# 중요: 실물은 lib/external에, x86_64-unix엔 반드시 '심링크'(복사 금지 — @loader_path 어긋나 assertion 루프)
mkdir -p "$ENGINE/lib/external"
cp -Rc "$CXG/D3DMetal.framework" "$CXG/libd3dshared.dylib" "$ENGINE/lib/external/" 2>/dev/null || true
GW="/Applications/CrossOver.app/Contents/SharedSupport/CrossOver/lib64/apple_gptk/wine"
cp -fc "$GW/x86_64-windows/"{d3d11,d3d12,dxgi}.dll "$ENGINE/lib/wine/x86_64-windows/" 2>/dev/null || true
( cd "$ENGINE/lib/wine/x86_64-unix" && for f in d3d10.so d3d11.so d3d12.so dxgi.so; do ln -sf ../../external/libd3dshared.dylib "$f"; done )
# wine-mono 10.4.1 (CrossOver에서 조달) — .NET 앱용
mkdir -p "$ENGINE/share/wine/mono"
cp -Rc "/Applications/CrossOver.app/Contents/SharedSupport/CrossOver/share/wine/mono/wine-mono-10.4.1" \
   "$ENGINE/share/wine/mono/" 2>/dev/null || true
# rpath + entitlement 서명 (Rosetta/실행메모리)
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
  codesign -f -s - --entitlements "$WORK/ent.plist" "$f" 2>/dev/null || true
done

echo "==> 완료: $ENGINE"
DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib" "$ENGINE/bin/wine" --version
echo
echo "실행 예:"
echo "  export WINEPREFIX=<보틀> CX_GRAPHICS_BACKEND=d3dmetal WINEMSYNC=1"
echo "  export DYLD_FALLBACK_LIBRARY_PATH='$ENGINE/lib:/usr/lib'"
echo "  '$ENGINE/bin/wine' 'C:\\\\Program Files (x86)\\\\Battle.net\\\\Battle.net Launcher.exe' --disable-gpu-compositing"
echo "  (Apple 보호 바이너리(nohup 등)를 체인에 두면 DYLD_* 가 제거되니 서브셸 &로 백그라운드 실행)"
