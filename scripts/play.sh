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
# 보틀(가상 C드라이브) 경로 — setup-bottle.sh로 만든 위치를 지정
export WINEPREFIX="${WINEPREFIX:-$HOME/.battlenet-macos/bottle}"

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

MODE="${1:-battlenet}"
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
  kill)        # 전부 종료
    "$ENGINE/bin/wineserver" -k 2>/dev/null || true
    ;;
  *) echo "usage: play.sh [battlenet|d2r|kill]"; exit 1;;
esac
