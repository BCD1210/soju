#!/usr/bin/env bash
# Steam(Windows판) 설치 — Steam판 D2R(Infernal Edition) 등 Steam 게임용.
#
# 중요: Steam은 Battle.net과 달리 CX 소스 엔진이 아니라 homebrew wine-stable로 돌린다.
# CX 계열 빌드에서는 Steam의 CEF UI가 렌더링되지 않는 문제(검은 화면)가 있고,
# 검증된 무료 조합은 wine-stable 11 + steamwebhelper 래퍼다.
# 래퍼 출처: github.com/notpop/steam-on-m1-wine (MIT) — third_party/ 에 소스+라이선스 동봉.
# 검증: 2026-08-27, M4 Pro / macOS 26.5 — 로그인 화면 렌더링 확인.
set -euo pipefail

export WINEPREFIX="${WINEPREFIX:-$HOME/.battlenet-macos/steam-bottle}"
SUPPORT="$HOME/.battlenet-macos/steam-support"
WORK="${WORK:-$HOME/.battlenet-macos/build}"; mkdir -p "$WORK" "$SUPPORT"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WINESTABLE_APP="/Applications/Wine Stable.app"
WINE="$WINESTABLE_APP/Contents/Resources/wine/bin/wine"

# ---------- 1. wine-stable ----------
if [ ! -x "$WINE" ]; then
  echo "==> homebrew wine-stable 설치 (무료, ~500MB)"
  # gstreamer-runtime 의존성이 sudo를 요구하므로 건너뛴다 (Steam UI에는 불필요)
  brew install --cask --skip-cask-deps wine-stable
  echo "==> Gatekeeper 격리 해제 (수 분 소요)"
  xattr -dr com.apple.quarantine "$WINESTABLE_APP" 2>/dev/null || true
fi
"$WINE" --version

# ---------- 2. webhelper 래퍼 ----------
# Steam의 CEF를 --disable-gpu --single-process로 강제해 검은 화면을 우회한다.
if [ ! -f "$SUPPORT/steamwebhelper-wrapper.exe" ]; then
  echo "==> 래퍼 빌드 (mingw-w64)"
  command -v x86_64-w64-mingw32-gcc >/dev/null || brew install mingw-w64
  x86_64-w64-mingw32-gcc -O2 -Wall -municode -DUNICODE -D_UNICODE \
    -o "$SUPPORT/steamwebhelper-wrapper.exe" \
    "$REPO_ROOT/third_party/steam-on-m1-wine/steamwebhelper-wrapper.c" \
    -static -lshell32 -mwindows
fi

# ---------- 3. 프리픽스 + Steam 설치 ----------
echo "==> 프리픽스 초기화: $WINEPREFIX"
"$WINE" wineboot -u >/dev/null 2>&1 || true
"$WINESTABLE_APP/Contents/Resources/wine/bin/wineserver" -w 2>/dev/null || true

SETUP="$WORK/SteamSetup.exe"
[ -f "$SETUP" ] || curl -fL "https://cdn.cloudflare.steamstatic.com/client/installer/SteamSetup.exe" -o "$SETUP"
echo "==> Steam 무인 설치"
"$WINE" "$SETUP" /S >/dev/null 2>&1 || true
"$WINESTABLE_APP/Contents/Resources/wine/bin/wineserver" -w 2>/dev/null || true
[ -f "$WINEPREFIX/drive_c/Program Files (x86)/Steam/steam.exe" ] || { echo "설치 실패"; exit 1; }

# ---------- 4. 마무리 ----------
cp -f /etc/ssl/cert.pem "$WINEPREFIX/drive_c/windows/cacert.pem" 2>/dev/null || true
echo "==> 완료. 실행: scripts/play.sh steam  (래퍼는 play.sh가 자동 배치)"
echo "    입력이 ?? 로 보이면 macOS 입력기를 영어(ABC)로 전환하세요."
