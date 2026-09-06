#!/usr/bin/env bash
# Enable D3D11 rendering for Steam games: wires DXMT + the patched winemac.
#
# Prerequisite: create-steam-bottle.sh. Verified components are downloaded into
# ~/.battlenet-macos/steam-support/ (dxmt-x64/{d3d11,dxgi,d3d10core,winemetal}.dll
# + winemetal.so, winemac-patched.so).
# See docs/STEAM-GAMES.md for how to build the artifacts (based on
# notpop/steam-on-m1-wine plus three fixes).
#
# Architecture (verified 2026-08-27, M4 Pro / macOS 26.5, Unity title rendering in-game):
#   - Bundle x86_64 builtins = DXMT (for games) / i386 builtins = untouched vanilla
#     (protects the 32-bit steam.exe composer)
#   - winemetal: bundle builtin + a "placeholder" copy in system32 (without it,
#     wine can't find the builtin by name: c0000135 -> the game's
#     "Failed to initialize graphics")
#   - system32 gets marker-stripped vanilla d3d dlls (native, Steam client only)
#   - Registry: global d3d11/d3d10core/dxgi/winemetal=builtin (DXMT);
#     steam.exe / steamwebhelper* / steamservice get per-app native (vanilla):
#     the modern Steam CEF conflicts with DXMT builtins and restart-loops, so the
#     split is mandatory
#   - winemac-patched.so: -fvisibility=default (DXMT dlsyms macdrv APIs) +
#     WINE_NO_DOCK_ICON support (single Dock icon)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BASE="${SOJU_BASE:-$HOME/.battlenet-macos}"
export WINEPREFIX="${WINEPREFIX:-$BASE/steam-bottle}"
source "$ROOT/scripts/steam-runtime.sh"
SUP="$BASE/steam-support/prebuilt"
python3 - "$ROOT/resources/steam-support.json" <<'PYVERSION'
import json, platform, sys
m=json.load(open(sys.argv[1]))
version=lambda s: tuple(map(int,s.split('.')))
if version(platform.mac_ver()[0]) < version(m['minimum_macos']):
    sys.exit('This Steam component build requires macOS '+m['minimum_macos']+' or later.')
PYVERSION
python3 "$ROOT/scripts/fetch-steam-support.py"
B="$WINEPREFIX/drive_c/windows"
[ -d "$B/system32" ] || { echo "Install Steam first: soju steam-install"; exit 1; }
unset DYLD_FALLBACK_LIBRARY_PATH WINEDLLOVERRIDES WINEMSYNC ROSETTA_ADVERTISE_AVX
unset CX_ACTIVE_GRAPHICS_BACKEND CX_GRAPHICS_BACKEND CX_APPLEGPTK_LIBD3DSHARED_PATH
export WINEDEBUG=-all
VERSION=$(cat "$SUP/.soju-support-version")
TARGET="$BASE/steam-runtime"
if [ "${1:-}" != --repair ] && [ -f "$TARGET/.soju-runtime" ] && \
   [ "$(cat "$WINEPREFIX/.soju-steam-games" 2>/dev/null || true)" = "$VERSION" ] && \
   [ "$(cat "$TARGET/.soju-runtime")" = "$VERSION" ]; then
  echo "Steam rendering is ready ($VERSION)."
  exit 0
fi
SOURCE="$STEAM_WINE_ROOT"
[ -x "$SOURCE/bin/wine" ] || { echo "Install wine-stable first: soju steam-install"; exit 1; }
[ "$("$SOURCE/bin/wine" --version)" = wine-11.0 ] || {
  echo "This Steam component release requires Wine 11.0. Existing runtime unchanged."; exit 1;
}
# Never migrate a live Wine Stable / private Steam session to another server.
if ps -axo comm= | grep -E '(Wine Stable.app|/steam-runtime)/.*wineserver$' >/dev/null; then
  echo "Close Windows Steam and its games, then run soju steam-games again."
  exit 1
fi
NEW=$(mktemp -d "$BASE/.steam-runtime.XXXXXX")
BACKUP=""; BACKUP_READY=0
rollback(){
  status=$?
  if [ "$status" != 0 ] && [ "$BACKUP_READY" = 1 ]; then
    "$NEW/bin/wineserver" -k 2>/dev/null || true
    "$NEW/bin/wineserver" -w 2>/dev/null || true
    cp "$BACKUP/user.reg" "$WINEPREFIX/user.reg"
    for D in d3d11 dxgi d3d10core winemetal; do
      if [ -f "$BACKUP/$D.dll" ]; then cp "$BACKUP/$D.dll" "$B/system32/$D.dll"
      else rm -f "$B/system32/$D.dll"; fi
    done
    if [ -f "$BACKUP/renderer-version" ]; then cp "$BACKUP/renderer-version" "$WINEPREFIX/.soju-steam-games"
    else rm -f "$WINEPREFIX/.soju-steam-games"; fi
    echo "Setup failed; the previous prefix configuration was restored."
  fi
  rm -rf "$NEW"
}
trap rollback EXIT
printf 'Preparing the Soju Steam runtime (existing Wine app is preserved)\n'
ditto "$SOURCE" "$NEW"
WAPP="$NEW"
WLIB="$WAPP/lib/wine"
WINE="$WAPP/bin/wine"

echo "==> 1/5 Backing up vanilla dlls (first run only)"
for D in d3d11 dxgi d3d10core; do
  [ -f "$WLIB/x86_64-windows/$D.dll.vanilla" ] || cp -f "$WLIB/x86_64-windows/$D.dll" "$WLIB/x86_64-windows/$D.dll.vanilla"
done

echo "==> 2/5 Wiring the private runtime: x86_64=DXMT, winemetal (builtin+unix), patched winemac"
cp -f "$SUP/dxmt-x64/"{d3d11.dll,dxgi.dll,d3d10core.dll,winemetal.dll} "$WLIB/x86_64-windows/"
cp -f "$SUP/dxmt-x64/winemetal.so" "$WLIB/x86_64-unix/"
[ -f "$WLIB/x86_64-unix/winemac.so.vanilla" ] || cp "$WLIB/x86_64-unix/winemac.so" "$WLIB/x86_64-unix/winemac.so.vanilla"
cp -f "$SUP/winemac-patched.so" "$WLIB/x86_64-unix/winemac.so"
# i386 stays vanilla (protects the 32-bit steam.exe composer)

BACKUP="$BASE/steam-support/prefix-backup-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$BACKUP"
cp "$WINEPREFIX/user.reg" "$BACKUP/user.reg"
for D in d3d11 dxgi d3d10core winemetal; do
  [ ! -f "$B/system32/$D.dll" ] || cp "$B/system32/$D.dll" "$BACKUP/$D.dll"
done
[ ! -f "$WINEPREFIX/.soju-steam-games" ] || cp "$WINEPREFIX/.soju-steam-games" "$BACKUP/renderer-version"
BACKUP_READY=1
echo "Recovery copy: $BACKUP"

echo "==> 3/5 Prefix: winemetal placeholder + marker-stripped vanilla natives for Steam"
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

echo "==> 4/5 Registry: global=builtin (DXMT), Steam processes per-app=native (vanilla)"
for DLL in d3d11 d3d10core dxgi winemetal; do
  "$WINE" reg add "HKCU\\Software\\Wine\\DllOverrides" /v "$DLL" /t REG_SZ /d builtin /f >/dev/null 2>&1
done
for APP in steam.exe steamwebhelper.exe steamwebhelper_real.exe steamservice.exe; do
  for DLL in d3d11 d3d10core dxgi; do
    "$WINE" reg add "HKCU\\Software\\Wine\\AppDefaults\\$APP\\DllOverrides" /v "$DLL" /t REG_SZ /d native /f >/dev/null 2>&1
  done
  "$WINE" reg add "HKCU\\Software\\Wine\\AppDefaults\\$APP\\DllOverrides" /v winemetal /t REG_SZ /d disabled /f >/dev/null 2>&1
done

echo "==> 5/5 Removing the fullscreen-forcing AppCompat token (allows windowed mode)"
# The reg adds above left a wineserver up with a dirty HKCU; it rewrites
# user.reg when it exits, which would undo an edit made now. Wait for it.
"$WAPP/bin/wineserver" -w 2>/dev/null || true
/usr/bin/python3 - "$WINEPREFIX/user.reg" <<'EOF'
import sys
p = sys.argv[1]
d = open(p, encoding='utf-8', errors='surrogateescape').read()
n = d.replace('DISABLEDXMAXIMIZEDWINDOWEDMODE', '')
if n != d: open(p, 'w', encoding='utf-8', errors='surrogateescape').write(n)
EOF

"$WINE" --version >/dev/null
printf '%s\n' "$VERSION" > "$NEW/.soju-runtime"
printf '%s\n' "$VERSION" > "$WINEPREFIX/.soju-steam-games"
[ ! -d "$TARGET.previous" ] || rm -rf "$TARGET.previous"
[ ! -d "$TARGET" ] || mv "$TARGET" "$TARGET.previous"
if ! mv "$NEW" "$TARGET"; then
  [ ! -d "$TARGET.previous" ] || mv "$TARGET.previous" "$TARGET"
  exit 1
fi
BACKUP_READY=0
echo "Done. Launch with scripts/play.sh steam, to pin windowed mode, use the"
echo "in-game setting or the Steam launch option '-screen-fullscreen 0' (Unity titles)."
