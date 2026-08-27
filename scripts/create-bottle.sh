#!/usr/bin/env bash
# CrossOver 없이 새 보틀을 만들고 Blizzard 공식 설치기로 Battle.net을 설치한다.
# 검증: 2026-08-27 — 순정 보틀에서 설치·로그인 화면까지 자동 완료 확인.
set -euo pipefail

ENGINE="${ENGINE:-$HOME/.battlenet-macos/cx26-engine}"
export WINEPREFIX="${WINEPREFIX:-$HOME/.battlenet-macos/bottle}"
export WINEDEBUG="${WINEDEBUG:-fixme-all}"
export WINEMSYNC=1 ROSETTA_ADVERTISE_AVX=1
export CX_APPLEGPTK_LIBD3DSHARED_PATH="$ENGINE/lib/external/libd3dshared.dylib"
export DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib"

[ -x "$ENGINE/bin/wine" ] || { echo "엔진 없음 — build-engine.sh 먼저"; exit 1; }
[ -f "$CX_APPLEGPTK_LIBD3DSHARED_PATH" ] || { echo "libd3dshared 없음 — get-gptk.sh 먼저"; exit 1; }

WORK="${WORK:-$HOME/.battlenet-macos/build}"; mkdir -p "$WORK"

echo "==> 프리픽스 초기화: $WINEPREFIX"
"$ENGINE/bin/wine" wineboot -u >/dev/null 2>&1 || true
"$ENGINE/bin/wineserver" -w

echo "==> Battle.net 설치기 다운로드 (Blizzard 공식)"
SETUP="$WORK/Battle.net-Setup.exe"
[ -f "$SETUP" ] || curl -fL "https://downloader.battle.net/download/getInstaller?os=win&installer=Battle.net-Setup.exe" -o "$SETUP"

echo "==> 설치 실행 (몇 분 소요, 자동 진행)"
"$ENGINE/bin/wine" "$SETUP" --lang=enUS >/dev/null 2>&1 || true

# 설치 완료 대기 (최대 10분)
for i in $(seq 1 60); do
  [ -f "$WINEPREFIX/drive_c/Program Files (x86)/Battle.net/Battle.net.exe" ] && break
  sleep 10
done
BN="$WINEPREFIX/drive_c/Program Files (x86)/Battle.net"
[ -f "$BN/Battle.net.exe" ] || { echo "설치 실패 — 로그: $WINEPREFIX/drive_c/ProgramData/Battle.net/Setup"; exit 1; }
echo "==> Battle.net 설치 완료"

"$ENGINE/bin/wineserver" -k 2>/dev/null || true
echo "==> 끝. 이제 실행: scripts/play.sh battlenet"
echo "    (게임은 배틀넷 로그인 후 앱 내에서 설치하면 됩니다)"
