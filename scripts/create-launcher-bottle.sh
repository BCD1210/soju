#!/usr/bin/env bash
# Create the bottle for the EA app, the Rockstar Games Launcher or Ubisoft Connect
# and run the publisher's official installer in it, on the launcher engine
# (scripts/launcher-engine.sh).
# Verified 2026-11-10 on an M4 Pro, macOS 26.5: each installer completes and the
# launcher opens its sign-in window. EA and Rockstar installers have a few
# buttons to press; Ubisoft's runs unattended.
set -euo pipefail
MODE="${1:?usage: create-launcher-bottle.sh ea|rockstar|ubisoft}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASE="${SOJU_BASE:-$HOME/.battlenet-macos}"
case "$MODE" in
  ea)       NAME="EA app"; CLIENT="Program Files/Electronic Arts/EA Desktop"
            URL="https://origin-a.akamaihd.net/EA-Desktop-Client-Download/installer-releases/EAappInstaller.exe"
            FILE="EAappInstaller.exe"; ARGS=() ;;
  rockstar) NAME="Rockstar Games Launcher"; CLIENT="Program Files/Rockstar Games/Launcher/Launcher.exe"
            URL="https://gamedownloads.rockstargames.com/public/installer/Rockstar-Games-Launcher.exe"
            FILE="Rockstar-Games-Launcher.exe"; ARGS=() ;;
  ubisoft)  NAME="Ubisoft Connect"; CLIENT="Program Files (x86)/Ubisoft/Ubisoft Game Launcher/UbisoftConnect.exe"
            URL="https://static3.cdn.ubi.com/orbit/launcher_installer/UbisoftConnectInstaller.exe"
            FILE="UbisoftConnectInstaller.exe"; ARGS=(/S) ;;
  *) echo "Unknown launcher: $MODE" >&2; exit 64 ;;
esac
export WINEPREFIX="${WINEPREFIX:-$BASE/$MODE-bottle}"
# The EA app lives in "EA Desktop\<version>\EA Desktop", reached through a
# junction ("EA Desktop\EA Desktop") that only Wine resolves and that the EA
# app's updater moves itself, so look for any installed version.
installed(){
  if [ "$MODE" = ea ]; then
    compgen -G "$WINEPREFIX/drive_c/$CLIENT/*/EA Desktop/EADesktop.exe" >/dev/null
  else
    [ -f "$WINEPREFIX/drive_c/$CLIENT" ]
  fi
}
if installed; then echo "==> $NAME already installed"; exit 0; fi

ENGINE=$(bash "$ROOT/scripts/launcher-engine.sh")
export WINEDEBUG="${WINEDEBUG:-fixme-all}" WINEMSYNC=1 ROSETTA_ADVERTISE_AVX=1 WINE_SIMULATE_WRITECOPY=1
export DXMT_ALLOW_CROSS_PROCESS_SWAPCHAIN=1
export CX_APPLEGPTK_LIBD3DSHARED_PATH="$ENGINE/lib/external/libd3dshared.dylib"
export DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib"
W="$ENGINE/bin/wine"
# winemenubuilder would turn their desktop shortcuts into Mac files, and it
# hangs on this engine; Soju makes its own app instead.
export WINEDLLOVERRIDES="${WINEDLLOVERRIDES:+$WINEDLLOVERRIDES;}winemenubuilder.exe="
WORK="$BASE/build"; mkdir -p "$WORK"

fetch_checked(){   # url, dest, sha256
  local got
  [ -f "$2" ] || { curl -fL --proto =https --retry 3 "$1" -o "$2.part" && mv -f "$2.part" "$2"; }
  got=$(shasum -a 256 "$2" | awk '{print $1}')
  [ "$got" = "$3" ] || { rm -f "$2"; echo "Checksum mismatch for $1" >&2; return 1; }
}

echo "==> Initializing the prefix: $WINEPREFIX"
"$W" wineboot -u >/dev/null 2>&1 || true
"$ENGINE/bin/wineserver" -w
"$W" reg add 'HKCU\Software\Wine\WineDbg' /v ShowCrashDialog /t REG_DWORD /d 0 /f >/dev/null 2>&1
# Their Windows services start slowly under Rosetta; Wine gives up on a service
# after 10 s by default (Rockstar's installer then waits forever).
"$W" reg add 'HKLM\System\CurrentControlSet\Control' /v ServicesPipeTimeout /t REG_SZ /d 60000 /f >/dev/null 2>&1
"$ENGINE/bin/wineserver" -w

if [ "$MODE" = ea ]; then
  # The EA installer's welcome page is HTML (mshtml). Without Wine Gecko Wine
  # first stops on a "Wine Gecko Installer" dialog, and declining it leaves the
  # page without its text. Same files and checksums Wine itself would fetch.
  echo "==> Installing Wine Gecko (the EA installer's welcome page needs it)"
  G="https://dl.winehq.org/wine/wine-gecko/2.47.4"
  if fetch_checked "$G/wine-gecko-2.47.4-x86_64.msi" "$WORK/wine-gecko-2.47.4-x86_64.msi" \
       e590b7d988a32d6aa4cf1d8aa3aa3d33766fdd4cf4c89c2dcc2095ecb28d066f \
     && fetch_checked "$G/wine-gecko-2.47.4-x86.msi" "$WORK/wine-gecko-2.47.4-x86.msi" \
       26cecc47706b091908f7f814bddb074c61beb8063318e9efc5a7f789857793d6; then
    "$W" msiexec /i "$WORK/wine-gecko-2.47.4-x86_64.msi" /qn >/dev/null 2>&1 || true
    "$W" msiexec /i "$WORK/wine-gecko-2.47.4-x86.msi" /qn >/dev/null 2>&1 || true
    "$ENGINE/bin/wineserver" -w
  else
    echo "    Wine Gecko could not be downloaded; if Wine offers to install it, press Install"
  fi
fi

echo "==> Downloading the $NAME installer (official)"
curl -fL --proto =https --retry 3 "$URL" -o "$WORK/$FILE.part" && mv -f "$WORK/$FILE.part" "$WORK/$FILE"

case "$MODE" in
  ubisoft) echo "==> Running the installer (unattended, about a minute)" ;;
  *) echo "==> Running the installer: follow its window (language, terms, Continue / Let's go)" ;;
esac
"$W" "$WORK/$FILE" ${ARGS[@]+"${ARGS[@]}"} >/dev/null 2>&1 || true
[ "$MODE" = ubisoft ] && "$ENGINE/bin/wineserver" -w

if ! installed; then
  echo "$NAME was not installed (the installer was closed or failed). Run it again with: soju $MODE-install" >&2
  exit 1
fi
echo "==> $NAME installed. Open it with: soju $MODE"
