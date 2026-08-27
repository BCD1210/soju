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

echo "==> 크래시 대화상자 무음화 (무해한 시작 크래시 2건이 창을 띄우지 않도록)"
# 배틀넷은 시작 시 보조 스레드 2개가 무해하게 죽는데, 기본 설정에선 매번 에러 창이 뜬다.
# 스레드를 조용히 동결시키는 디버거로 교체 (상세: docs/DIAGNOSIS.md)
W="$ENGINE/bin/wine"
"$W" reg add "HKCU\\Software\\Wine\\WineDbg" /v ShowCrashDialog /t REG_DWORD /d 0 /f >/dev/null 2>&1
for HIVE in "HKLM\\Software\\Microsoft\\Windows NT\\CurrentVersion\\AeDebug" \
            "HKLM\\Software\\Wow6432Node\\Microsoft\\Windows NT\\CurrentVersion\\AeDebug"; do
  "$W" reg add "$HIVE" /v Debugger /t REG_SZ /d "C:\\windows\\system32\\rundll32.exe kernel32.dll,Sleep" /f >/dev/null 2>&1
  "$W" reg add "$HIVE" /v Auto /t REG_SZ /d 1 /f >/dev/null 2>&1
done
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
