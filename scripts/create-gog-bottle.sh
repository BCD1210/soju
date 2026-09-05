#!/usr/bin/env bash
# Create a bottle for GOG GALAXY on the free CX-source engine and install it
# with GOG's official web installer.
# Verified 2026-08-30 (M4 Pro / macOS 26.5): install, login window, login.
# GOG GALAXY 2.x is a Qt6 + QtWebEngine app, not CEF. Its window stays black
# on this engine unless Chromium runs with --disable-gpu, and GOG overwrites
# QTWEBENGINE_CHROMIUM_FLAGS itself and ignores its own argv, so the engine
# carries a small hook (patches/chromium-flags-append.patch) that appends
# SOJU_CHROMIUM_FLAGS to that variable whenever a program sets it. play.sh gog
# sets it. See docs/DIAGNOSIS.md.
set -euo pipefail

ENGINE="${ENGINE:-$HOME/.battlenet-macos/cx26-engine}"
export WINEPREFIX="${WINEPREFIX:-$HOME/.battlenet-macos/gog-bottle}"
export WINEDEBUG="${WINEDEBUG:-fixme-all}"
export WINEMSYNC=1 ROSETTA_ADVERTISE_AVX=1 WINE_SIMULATE_WRITECOPY=1
export CX_APPLEGPTK_LIBD3DSHARED_PATH="$ENGINE/lib/external/libd3dshared.dylib"
export DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib"

# A previous run may have installed the client but failed to build its helper.
if [ -f "$WINEPREFIX/drive_c/Program Files/GOG Galaxy/GalaxyClient.exe" ]; then
  bash "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/ensure-launcher-helper.sh" gog
  echo "==> Launcher already installed; tray-restore helper ready"
  exit 0
fi

[ -x "$ENGINE/bin/wine" ] || { echo "Engine not found, run install.sh (or build-engine.sh) first"; exit 1; }
[ -f "$CX_APPLEGPTK_LIBD3DSHARED_PATH" ] || { echo "libd3dshared not found, run get-gptk.sh first"; exit 1; }

WORK="${WORK:-$HOME/.battlenet-macos/build}"; mkdir -p "$WORK"
W="$ENGINE/bin/wine"

echo "==> Initializing the prefix: $WINEPREFIX"
"$W" wineboot -u >/dev/null 2>&1 || true
"$ENGINE/bin/wineserver" -w
"$W" reg add "HKCU\\Software\\Wine\\WineDbg" /v ShowCrashDialog /t REG_DWORD /d 0 /f >/dev/null 2>&1
"$ENGINE/bin/wineserver" -w

echo "==> Downloading the GOG GALAXY installer"
EXE="$WORK/setup_galaxy.exe"
[ -f "$EXE" ] || { curl -fL "https://webinstallers.gog-statics.com/download/GOG_Galaxy_2.0.exe" -o "$EXE.part" && mv -f "$EXE.part" "$EXE"; }

echo "==> Running the installer (silent; it downloads the client, a few minutes)"
"$W" "$EXE" /VERYSILENT /NORESTART /SUPPRESSMSGBOXES >/dev/null 2>&1 || true
"$ENGINE/bin/wineserver" -w 2>/dev/null || true

CLIENT="$WINEPREFIX/drive_c/Program Files/GOG Galaxy/GalaxyClient.exe"
[ -f "$CLIENT" ] || { echo "Install failed, try running the installer by hand: $W \"$EXE\""; exit 1; }
# The installer auto-starts the client with an empty payload, which makes it
# exit with a config error and leave a lock file behind. Stop it and clean up.
"$ENGINE/bin/wineserver" -k 2>/dev/null || true
sleep 2
rm -f "$WINEPREFIX/drive_c/ProgramData/GOG.com/Galaxy/lock-files/"* 2>/dev/null || true
bash "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/ensure-launcher-helper.sh" gog
echo "==> GOG GALAXY installed. Launch with: scripts/play.sh gog"
