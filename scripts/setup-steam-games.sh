#!/usr/bin/env bash
# Enable D3D11 rendering for Steam games: wires DXMT + the patched winemac.
#
# Prerequisites: create-steam-bottle.sh done, and the DXMT artifacts present in
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

SUP="$HOME/.battlenet-macos/steam-support"
export WINEPREFIX="${WINEPREFIX:-$HOME/.battlenet-macos/steam-bottle}"
WAPP="/Applications/Wine Stable.app/Contents/Resources/wine"
WLIB="$WAPP/lib/wine"
WINE="$WAPP/bin/wine"
B="$WINEPREFIX/drive_c/windows"

[ -x "$WINE" ] || { echo "wine-stable not found, run create-steam-bottle.sh first"; exit 1; }
for f in dxmt-x64/d3d11.dll dxmt-x64/dxgi.dll dxmt-x64/d3d10core.dll dxmt-x64/winemetal.dll dxmt-x64/winemetal.so winemac-patched.so; do
  [ -f "$SUP/$f" ] || { echo "Missing artifact: $SUP/$f, see the build steps in docs/STEAM-GAMES.md"; exit 1; }
done

echo "==> 1/5 Backing up vanilla dlls (first run only)"
for D in d3d11 dxgi d3d10core; do
  [ -f "$WLIB/x86_64-windows/$D.dll.vanilla" ] || cp -f "$WLIB/x86_64-windows/$D.dll" "$WLIB/x86_64-windows/$D.dll.vanilla"
done

echo "==> 2/5 Wiring the bundle: x86_64=DXMT, winemetal (builtin+unix), patched winemac"
cp -f "$SUP/dxmt-x64/"{d3d11.dll,dxgi.dll,d3d10core.dll,winemetal.dll} "$WLIB/x86_64-windows/"
cp -f "$SUP/dxmt-x64/winemetal.so" "$WLIB/x86_64-unix/"
cp -f "$SUP/winemac-patched.so" "$WLIB/x86_64-unix/winemac.so"
# i386 stays vanilla (protects the 32-bit steam.exe composer)

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

echo "Done. Launch with scripts/play.sh steam, to pin windowed mode, use the"
echo "in-game setting or the Steam launch option '-screen-fullscreen 0' (Unity titles)."
