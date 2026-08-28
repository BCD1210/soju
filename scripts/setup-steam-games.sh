#!/usr/bin/env bash
# Steam 게임(D3D11) 렌더링 활성화 — DXMT + 패치 winemac 배선.
#
# 전제: create-steam-bottle.sh 완료 + DXMT 아티팩트가 ~/.battlenet-macos/steam-support/
# 에 존재 (dxmt-x64/{d3d11,dxgi,d3d10core,winemetal}.dll + winemetal.so, winemac-patched.so).
# 아티팩트 빌드법은 docs/STEAM-GAMES.md 참고 (notpop/steam-on-m1-wine 기반 + 3가지 픽스).
#
# 아키텍처 (검증: 2026-08-27, M4 Pro / macOS 26.5, Unity 게임 인게임 렌더링 확인):
#   - 번들 x86_64 빌트인 = DXMT (게임용) / i386 빌트인 = 순정 유지 (32비트 steam.exe 보호)
#   - winemetal: 번들 빌트인 + system32 "플레이스홀더" 사본 (없으면 wine이 빌트인을
#     이름으로 못 찾아 c0000135 → 게임 "Failed to initialize graphics")
#   - system32에 "마커 제거" 순정 d3d dll (Steam 클라이언트 전용 native)
#   - 레지스트리: 전역 d3d11/d3d10core/dxgi/winemetal=builtin(DXMT),
#     steam.exe·steamwebhelper*·steamservice per-app=native(순정) — 신형 Steam CEF는
#     DXMT 빌트인과 충돌해 무한 재시작하므로 반드시 분리
#   - winemac-patched.so: -fvisibility=default (DXMT가 macdrv API를 dlsym) +
#     WINE_NO_DOCK_ICON 지원 (독 아이콘 단일화)
set -euo pipefail

SUP="$HOME/.battlenet-macos/steam-support"
export WINEPREFIX="${WINEPREFIX:-$HOME/.battlenet-macos/steam-bottle}"
WAPP="/Applications/Wine Stable.app/Contents/Resources/wine"
WLIB="$WAPP/lib/wine"
WINE="$WAPP/bin/wine"
B="$WINEPREFIX/drive_c/windows"

[ -x "$WINE" ] || { echo "wine-stable 없음 — create-steam-bottle.sh 먼저"; exit 1; }
for f in dxmt-x64/d3d11.dll dxmt-x64/dxgi.dll dxmt-x64/d3d10core.dll dxmt-x64/winemetal.dll dxmt-x64/winemetal.so winemac-patched.so; do
  [ -f "$SUP/$f" ] || { echo "아티팩트 없음: $SUP/$f — docs/STEAM-GAMES.md의 빌드 절차 참고"; exit 1; }
done

echo "==> 1/5 순정 dll 백업 (최초 1회)"
for D in d3d11 dxgi d3d10core; do
  [ -f "$WLIB/x86_64-windows/$D.dll.vanilla" ] || cp -f "$WLIB/x86_64-windows/$D.dll" "$WLIB/x86_64-windows/$D.dll.vanilla"
done

echo "==> 2/5 번들 배선: x86_64=DXMT, winemetal(빌트인+unix), 패치 winemac"
cp -f "$SUP/dxmt-x64/"{d3d11.dll,dxgi.dll,d3d10core.dll,winemetal.dll} "$WLIB/x86_64-windows/"
cp -f "$SUP/dxmt-x64/winemetal.so" "$WLIB/x86_64-unix/"
cp -f "$SUP/winemac-patched.so" "$WLIB/x86_64-unix/winemac.so"
# i386은 순정 그대로 (32비트 steam.exe 컴포저 보호)

echo "==> 3/5 프리픽스: winemetal 플레이스홀더 + Steam 전용 순정 native(마커 제거)"
cp -f "$WLIB/x86_64-windows/winemetal.dll" "$B/system32/winemetal.dll"
for D in d3d11 dxgi d3d10core; do
  cp -f "$WLIB/x86_64-windows/$D.dll.vanilla" "$B/system32/$D.dll"
  /usr/bin/python3 - "$B/system32/$D.dll" <<'EOF'
import sys
p = sys.argv[1]; d = open(p,'rb').read()
m = b'Wine builtin DLL'
if m in d: open(p,'wb').write(d.replace(m, b'Xine builtin DLL', 1))
EOF
done

echo "==> 4/5 레지스트리: 전역=builtin(DXMT), Steam 계열 per-app=native(순정)"
for DLL in d3d11 d3d10core dxgi winemetal; do
  "$WINE" reg add "HKCU\\Software\\Wine\\DllOverrides" /v "$DLL" /t REG_SZ /d builtin /f >/dev/null 2>&1
done
for APP in steam.exe steamwebhelper.exe steamwebhelper_real.exe steamservice.exe; do
  for DLL in d3d11 d3d10core dxgi; do
    "$WINE" reg add "HKCU\\Software\\Wine\\AppDefaults\\$APP\\DllOverrides" /v "$DLL" /t REG_SZ /d native /f >/dev/null 2>&1
  done
  "$WINE" reg add "HKCU\\Software\\Wine\\AppDefaults\\$APP\\DllOverrides" /v winemetal /t REG_SZ /d disabled /f >/dev/null 2>&1
done

echo "==> 5/5 전체화면 강제 AppCompat 토큰 제거 (창모드 허용)"
/usr/bin/python3 - "$WINEPREFIX/user.reg" <<'EOF'
import sys
p = sys.argv[1]
d = open(p, encoding='utf-8', errors='surrogateescape').read()
n = d.replace('DISABLEDXMAXIMIZEDWINDOWEDMODE', '')
if n != d: open(p, 'w', encoding='utf-8', errors='surrogateescape').write(n)
EOF

echo "완료. scripts/play.sh steam 으로 실행 — 게임 창모드 고정은 게임 내 설정 또는"
echo "Steam 시작 옵션 '-screen-fullscreen 0' (Unity 계열) 사용."
