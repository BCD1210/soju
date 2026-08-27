#!/usr/bin/env bash
# Apple Game Porting Toolkit에서 libd3dshared + D3DMetal.framework를 엔진에 설치한다.
#
# 왜 필요한가: libd3dshared는 D2R 로더(안티치트)가 Rosetta 2를 통과하는 데 필수다
# (비네이티브 코드영역 등록). 그래픽(D3DMetal)도 이걸로 제공된다. Apple이 재배포를
# 금지하므로 사용자가 직접 받아야 한다 — 무료 Apple 계정이면 된다.
#
# 1) https://developer.apple.com/games/game-porting-toolkit/ 에서
#    "Game Porting Toolkit" dmg 다운로드 (Apple ID 로그인 필요, 무료)
# 2) dmg를 연 상태에서(또는 경로 지정) 이 스크립트 실행:
#      scripts/get-gptk.sh                  # 마운트된 볼륨 자동 탐색
#      scripts/get-gptk.sh /path/to/GPTK    # 직접 경로 지정
#
# 대안: CrossOver(체험판 포함)가 설치돼 있으면 거기서 자동 추출한다.
set -euo pipefail

ENGINE="${ENGINE:-$HOME/.battlenet-macos/cx26-engine}"
[ -d "$ENGINE/lib" ] || { echo "엔진이 없습니다. 먼저 build-engine.sh 실행."; exit 1; }

find_payload() {
  local roots=("$@")
  for r in "${roots[@]}"; do
    [ -d "$r" ] || continue
    local f
    f=$(find "$r" -maxdepth 6 -name "libd3dshared.dylib" 2>/dev/null | head -1)
    [ -n "$f" ] && { echo "$(dirname "$f")"; return 0; }
  done
  return 1
}

SRC=""
if [ $# -ge 1 ]; then
  SRC=$(find_payload "$1") || true
fi
if [ -z "$SRC" ]; then
  # 마운트된 GPTK dmg 자동 탐색
  SRC=$(find_payload /Volumes/Game* /Volumes/*orting* 2>/dev/null) || true
fi
if [ -z "$SRC" ]; then
  # CrossOver 설치본에서 추출 (대안 경로)
  SRC=$(find_payload "/Applications/CrossOver.app/Contents/SharedSupport/CrossOver/lib64/apple_gptk") || true
fi
if [ -z "$SRC" ]; then
  echo "libd3dshared.dylib를 찾지 못했습니다."
  echo "GPTK dmg를 마운트했는지, 또는 경로 인자를 확인하세요."
  exit 1
fi

echo "==> GPTK 페이로드: $SRC"
mkdir -p "$ENGINE/lib/external"
cp -Rf "$SRC/D3DMetal.framework" "$ENGINE/lib/external/" 2>/dev/null || true
cp -f  "$SRC/libd3dshared.dylib" "$ENGINE/lib/external/"
# 심링크 배선 (복사 금지! @loader_path 규칙)
( cd "$ENGINE/lib/wine/x86_64-unix" && for f in d3d10.so d3d11.so d3d12.so dxgi.so; do ln -sf ../../external/libd3dshared.dylib "$f"; done )
echo "==> 설치 완료: $ENGINE/lib/external"
echo "    (D3DMetal.framework가 없고 libd3dshared만 있어도 게임은 vkd3d 그래픽으로 동작합니다)"
