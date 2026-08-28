#!/usr/bin/env bash
# 디아블로 II 레저렉션 — 완전 무료 스택 실행 스크립트 (최종 검증 조합)
#
# 구성: 자체 빌드 wine 11.0 (CrossOver 26.3 GPL 소스) + D3DMetal(Apple GPTK)
# 검증: 2026-08-27 — Battle.net 로그인/Agent/D2R 인게임 구동 확인
#
# 핵심 열쇠 3개 (이걸 몰라서 며칠 걸렸다):
#  1) ROSETTA_ADVERTISE_AVX=1   — D2R 로더가 AVX 명령어 필수. 없으면 무한 대기(86MB 정지).
#  2) D3DMetal 심링크 레이아웃  — lib/external에 실물, x86_64-unix엔 심링크.
#     복사하면 @loader_path가 어긋나 assertion 루프. (frankea README 참고)
#  3) Apple 보호 바이너리(nohup/arch 등)를 실행 체인에 두지 말 것 — DYLD_* 가 제거됨.
set -euo pipefail

ENGINE="${ENGINE:-$HOME/.battlenet-macos/cx26-engine}"
MODE="${1:-battlenet}"

# 보틀(가상 C드라이브) 경로 — 모드별 기본값 (Battle.net과 Steam은 별도 보틀)
if [[ "$MODE" == steam* ]]; then
  export WINEPREFIX="${WINEPREFIX:-$HOME/.battlenet-macos/steam-bottle}"
else
  export WINEPREFIX="${WINEPREFIX:-$HOME/.battlenet-macos/bottle}"
fi

export WINEDEBUG="${WINEDEBUG:-fixme-all}"
export WINEMSYNC=1
export ROSETTA_ADVERTISE_AVX=1
export CX_ACTIVE_GRAPHICS_BACKEND=d3dmetal
export CX_GRAPHICS_BACKEND=d3dmetal
export CX_APPLEGPTK_LIBD3DSHARED_PATH="$ENGINE/lib/external/libd3dshared.dylib"
export DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib"

# Agent 서명검증 fix: 버전 하위폴더에 서명된 exe 동기화
BN="$WINEPREFIX/drive_c/Program Files (x86)/Battle.net"
if [[ -f "$BN/Battle.net.exe" ]]; then
  for v in "$BN"/Battle.net.[0-9]*; do
    [[ -d "$v" ]] && cp -cf "$BN/Battle.net.exe" "$v/Battle.net.exe" 2>/dev/null || true
  done
fi

case "$MODE" in
  battlenet)   # 배틀넷 런처 (로그인 → Play로 온라인 플레이)
    exec "$ENGINE/bin/wine" \
      "C:\\Program Files (x86)\\Battle.net\\Battle.net Launcher.exe" \
      --disable-gpu-compositing
    ;;
  d2r)         # 게임 직접 실행 (오프라인/이전 세션)
    cd "$WINEPREFIX/drive_c/Program Files (x86)/Diablo II Resurrected"
    exec "$ENGINE/bin/wine" "D2R.exe" "${@:2}"
    ;;
  steam)       # Steam 클라이언트 — wine-stable + webhelper 래퍼 (검증: 2026-08-27)
    # CX 엔진에선 Steam CEF가 렌더링되지 않는다 (검은 화면 / SEGV 폭풍).
    # 검증된 조합은 homebrew wine-stable 11 + steamwebhelper 래퍼(--disable-gpu
    # --single-process 강제) + -no-cef-sandbox -cef-single-process.
    # 출처: github.com/notpop/steam-on-m1-wine (MIT) — third_party/ 참고.
    WINESTABLE="/Applications/Wine Stable.app/Contents/Resources/wine/bin/wine"
    [[ -x "$WINESTABLE" ]] || { echo "wine-stable 없음 — brew install --cask wine-stable"; exit 1; }
    ST="$WINEPREFIX/drive_c/Program Files (x86)/Steam/steam.exe"
    [[ -f "$ST" ]] || { echo "Steam 없음 — scripts/create-steam-bottle.sh 먼저"; exit 1; }
    # 1) 크래시 잔여물 청소 (남으면 다음 실행이 창 없는 --silent 모드로 빠짐)
    find "$WINEPREFIX/drive_c/users/"*/AppData/Local/Steam/htmlcache -maxdepth 2 \
      \( -name "Singleton*" -o -name "*.lock" \) -delete 2>/dev/null || true
    # 2) 래퍼 재배치 (Steam 업데이트가 래퍼를 원본으로 되돌리므로 매번 확인)
    WRAP="$HOME/.battlenet-macos/steam-support/steamwebhelper-wrapper.exe"
    for d in "$WINEPREFIX/drive_c/Program Files (x86)/Steam/bin/cef"/cef.win*; do
      [[ -f "$d/steamwebhelper.exe" ]] || continue
      if [[ $(stat -f%z "$d/steamwebhelper.exe") -gt 500000 ]]; then
        mv -f "$d/steamwebhelper.exe" "$d/steamwebhelper_real.exe"
        cp -f "$WRAP" "$d/steamwebhelper.exe"
      fi
    done
    # 3) 실행 — CX 엔진 env 없이 wine-stable 순정 환경
    env -u DYLD_FALLBACK_LIBRARY_PATH -u CX_ACTIVE_GRAPHICS_BACKEND -u CX_GRAPHICS_BACKEND \
        -u CX_APPLEGPTK_LIBD3DSHARED_PATH -u WINEMSYNC -u ROSETTA_ADVERTISE_AVX \
      WINEPREFIX="$WINEPREFIX" WINEDEBUG="${WINEDEBUG:-fixme-all}" \
      WINEDLLOVERRIDES="d3d11,d3d10core,dxgi=b;winemetal=d;bcrypt=b;ncrypt=b;gameoverlayrenderer,gameoverlayrenderer64=d" \
      "$WINESTABLE" "C:\\Program Files (x86)\\Steam\\steam.exe" \
      -no-cef-sandbox -cef-single-process -noverifyfiles "${@:2}"
    ;;
  kill|steam-kill)  # 종료 (kill=배틀넷 보틀, steam-kill=Steam 보틀)
    "$ENGINE/bin/wineserver" -k 2>/dev/null || true
    ;;
  *) echo "usage: play.sh [battlenet|d2r|steam|kill|steam-kill]"; exit 1;;
esac
