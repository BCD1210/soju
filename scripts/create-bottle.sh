#!/usr/bin/env bash
# Create a fresh bottle without CrossOver and install Battle.net with Blizzard's
# official installer.
# Verified 2026-08-27: automatic install through to the login screen on a pristine bottle.
set -euo pipefail

ENGINE="${ENGINE:-$HOME/.battlenet-macos/cx26-engine}"
export WINEPREFIX="${WINEPREFIX:-$HOME/.battlenet-macos/bottle}"
export WINEDEBUG="${WINEDEBUG:-fixme-all}"
export WINEMSYNC=1 ROSETTA_ADVERTISE_AVX=1
export CX_APPLEGPTK_LIBD3DSHARED_PATH="$ENGINE/lib/external/libd3dshared.dylib"
export DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib"

[ -x "$ENGINE/bin/wine" ] || { echo "Engine not found — run build-engine.sh first"; exit 1; }
[ -f "$CX_APPLEGPTK_LIBD3DSHARED_PATH" ] || { echo "libd3dshared not found — run get-gptk.sh first"; exit 1; }

WORK="${WORK:-$HOME/.battlenet-macos/build}"; mkdir -p "$WORK"

echo "==> Initializing the prefix: $WINEPREFIX"
"$ENGINE/bin/wine" wineboot -u >/dev/null 2>&1 || true
"$ENGINE/bin/wineserver" -w

echo "==> Silencing crash dialogs (two harmless startup crashes would pop windows)"
# Battle.net harmlessly loses two helper threads at startup; by default each one
# pops an error dialog. Swap in a debugger that quietly freezes the thread
# (details: docs/DIAGNOSIS.md).
W="$ENGINE/bin/wine"
"$W" reg add "HKCU\\Software\\Wine\\WineDbg" /v ShowCrashDialog /t REG_DWORD /d 0 /f >/dev/null 2>&1
for HIVE in "HKLM\\Software\\Microsoft\\Windows NT\\CurrentVersion\\AeDebug" \
            "HKLM\\Software\\Wow6432Node\\Microsoft\\Windows NT\\CurrentVersion\\AeDebug"; do
  "$W" reg add "$HIVE" /v Debugger /t REG_SZ /d "C:\\windows\\system32\\rundll32.exe kernel32.dll,Sleep" /f >/dev/null 2>&1
  "$W" reg add "$HIVE" /v Auto /t REG_SZ /d 1 /f >/dev/null 2>&1
done
"$ENGINE/bin/wineserver" -w

echo "==> Downloading the Battle.net installer (official Blizzard)"
SETUP="$WORK/Battle.net-Setup.exe"
[ -f "$SETUP" ] || curl -fL "https://downloader.battle.net/download/getInstaller?os=win&installer=Battle.net-Setup.exe" -o "$SETUP"

echo "==> Running the installer (a few minutes, fully automatic)"
"$ENGINE/bin/wine" "$SETUP" --lang=enUS >/dev/null 2>&1 || true

# Wait for the install to finish (up to 10 minutes)
for i in $(seq 1 60); do
  [ -f "$WINEPREFIX/drive_c/Program Files (x86)/Battle.net/Battle.net.exe" ] && break
  sleep 10
done
BN="$WINEPREFIX/drive_c/Program Files (x86)/Battle.net"
[ -f "$BN/Battle.net.exe" ] || { echo "Install failed — logs: $WINEPREFIX/drive_c/ProgramData/Battle.net/Setup"; exit 1; }
echo "==> Battle.net installed"

"$ENGINE/bin/wineserver" -k 2>/dev/null || true
echo "==> Done. Now run: scripts/play.sh battlenet"
echo "    (Install games inside the app after logging in to Battle.net)"
