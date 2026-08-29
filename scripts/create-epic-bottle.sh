#!/usr/bin/env bash
# Create a bottle for the Epic Games Launcher on the free CX-source engine and
# install it with Epic's official MSI.
# Verified 2026-08-29 (M4 Pro / macOS 26.5): unattended install (~30 s), launcher
# UI renders, login works. No launcher-specific hacks were needed beyond the
# Battle.net environment — Epic's CEF (EpicWebHelper) keeps its GPU process
# alive on this engine, so no --in-process-gpu switch either.
set -euo pipefail

ENGINE="${ENGINE:-$HOME/.battlenet-macos/cx26-engine}"
export WINEPREFIX="${WINEPREFIX:-$HOME/.battlenet-macos/epic-bottle}"
export WINEDEBUG="${WINEDEBUG:-fixme-all}"
export WINEMSYNC=1 ROSETTA_ADVERTISE_AVX=1 WINE_SIMULATE_WRITECOPY=1
export CX_APPLEGPTK_LIBD3DSHARED_PATH="$ENGINE/lib/external/libd3dshared.dylib"
export DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib"

[ -x "$ENGINE/bin/wine" ] || { echo "Engine not found — run install.sh (or build-engine.sh) first"; exit 1; }
[ -f "$CX_APPLEGPTK_LIBD3DSHARED_PATH" ] || { echo "libd3dshared not found — run get-gptk.sh first"; exit 1; }

WORK="${WORK:-$HOME/.battlenet-macos/build}"; mkdir -p "$WORK"
W="$ENGINE/bin/wine"

echo "==> Initializing the prefix: $WINEPREFIX"
"$W" wineboot -u >/dev/null 2>&1 || true
"$ENGINE/bin/wineserver" -w
"$W" reg add "HKCU\\Software\\Wine\\WineDbg" /v ShowCrashDialog /t REG_DWORD /d 0 /f >/dev/null 2>&1
"$ENGINE/bin/wineserver" -w

echo "==> Downloading the Epic Games Launcher installer (official Epic MSI)"
MSI="$WORK/EpicGamesLauncherInstaller.msi"
[ -f "$MSI" ] || curl -fL "https://launcher-public-service-prod06.ol.epicgames.com/launcher/api/installer/download/EpicGamesLauncherInstaller.msi" -o "$MSI"

echo "==> Running the installer (unattended, about half a minute)"
"$W" msiexec /i "$MSI" /qn >/dev/null 2>&1 || true
"$ENGINE/bin/wineserver" -w

EXE="$WINEPREFIX/drive_c/Program Files/Epic Games/Launcher/Portal/Binaries/Win64/EpicGamesLauncher.exe"
[ -f "$EXE" ] || { echo "Install failed — try: WINEDEBUG=+msi $W msiexec /i \"$MSI\""; exit 1; }
echo "==> Epic Games Launcher installed"
"$ENGINE/bin/wineserver" -k 2>/dev/null || true
echo "==> Done. Now run: scripts/play.sh epic   (log in, then install games from the launcher)"
