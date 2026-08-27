#!/usr/bin/env bash
# 기존 CrossOver "Battle.net Desktop App" 보틀을 새 보틀로 완전 복제한다.
# 게임 파일만 옮기면 CEF/Agent가 오작동 → 레지스트리(system/user/userdef)까지 통째 복제해야 정상.
# APFS 클론(cp -c)이라 디스크 추가 사용 거의 없음.
set -euo pipefail

CX_BOTTLE="${CX_BOTTLE:-$HOME/Library/Application Support/CrossOver/Bottles/Battle.net Desktop App}"
DEST="${DEST:?새 보틀 경로 DEST를 지정하세요 (예: ~/BattleNetBottle)}"

if [[ ! -d "$CX_BOTTLE/drive_c" ]]; then
  echo "CrossOver 보틀을 찾을 수 없음: $CX_BOTTLE" >&2
  echo "게임이 설치된 CrossOver 보틀 경로를 CX_BOTTLE로 지정하세요." >&2
  exit 1
fi

echo "==> 보틀 복제: $CX_BOTTLE -> $DEST"
mkdir -p "$DEST"
cp -Rc "$CX_BOTTLE/drive_c" "$DEST/"
for f in system.reg user.reg userdef.reg; do
  [[ -f "$CX_BOTTLE/$f" ]] && cp -c "$CX_BOTTLE/$f" "$DEST/"
done
[[ -d "$CX_BOTTLE/dosdevices" ]] && cp -Rc "$CX_BOTTLE/dosdevices" "$DEST/"

echo "==> 완료. 실행:"
echo "    WINEPREFIX='$DEST' scripts/launch.sh"
