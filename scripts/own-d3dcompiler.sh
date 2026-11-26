#!/usr/bin/env bash
# Make games that compile shaders while they load use the D3DCompiler they ship.
#
# CrossOver's Wine loads its own d3dcompiler_47.dll even when the game has Microsoft's next
# to its exe. Diablo Immortal compiles its shaders with it when entering the world; with
# Wine's compiler next to none come out (LocalData/Cache3/LocalShaders/compiled.dx11 stays
# near 17 KB instead of 300 KB) and the game waits on the loading screen at 100% for good,
# at a few percent CPU (#221, seen on M4 Pro and M5). With the game's own DLL the world
# opens in under half a minute.
#
#   own-d3dcompiler.sh           needs WINEPREFIX and ENGINE (and the Wine environment of play.sh)
#
# A key that already exists is left alone, so a value set by hand stays. Reads user.reg
# instead of asking Wine, so a start that has nothing to add costs no Wine process.
set -u
GAMES="DiabloImmortal.exe"
DLLS="d3dcompiler_47 d3dcompiler_43"
[ -n "${WINEPREFIX:-}" ] && [ -x "${ENGINE:-}/bin/wine" ] || exit 0
for exe in $GAMES; do
  if grep -qiF "[Software\\\\Wine\\\\AppDefaults\\\\$exe\\\\DllOverrides]" "$WINEPREFIX/user.reg" 2>/dev/null; then
    continue
  fi
  for dll in $DLLS; do
    "$ENGINE/bin/wine" reg add "HKCU\\Software\\Wine\\AppDefaults\\$exe\\DllOverrides" /v "$dll" /t REG_SZ /d native /f >/dev/null 2>&1 || true
  done
done
exit 0
