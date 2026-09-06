#!/usr/bin/env bash
# Install the Windows Steam client and its verified rendering components.
#
# Steam uses upstream Wine 11 plus the steamwebhelper wrapper and DXMT.
# New installs use a checksum-pinned private runtime; legacy Wine apps are kept.
set -euo pipefail

BASE="${SOJU_BASE:-$HOME/.battlenet-macos}"
export WINEPREFIX="${WINEPREFIX:-$BASE/steam-bottle}"
WORK="${WORK:-$BASE/build}"; mkdir -p "$WORK"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
source "$REPO_ROOT/scripts/steam-runtime.sh"
# ---------- 1. Pinned upstream Wine ----------
python3 - "$REPO_ROOT/resources/steam-support.json" <<'PYVERSION'
import json, platform, sys
m=json.load(open(sys.argv[1]))
version=lambda s: tuple(map(int,s.split('.')))
if version(platform.mac_ver()[0]) < version(m['minimum_macos']):
    sys.exit('Steam game support requires macOS '+m['minimum_macos']+' or later.')
PYVERSION
unset DYLD_FALLBACK_LIBRARY_PATH WINEDLLOVERRIDES WINEMSYNC ROSETTA_ADVERTISE_AVX
unset CX_ACTIVE_GRAPHICS_BACKEND CX_GRAPHICS_BACKEND CX_APPLEGPTK_LIBD3DSHARED_PATH WINE_SIMULATE_WRITECOPY
WINE="$STEAM_WINE_ROOT/bin/wine"
if [ ! -x "$WINE" ] || [ "$("$WINE" --version 2>/dev/null || true)" != wine-11.0 ]; then
  if [ -n "${SOJU_STEAM_WINE:-}" ]; then
    echo "SOJU_STEAM_WINE must point to a working Wine 11.0 runtime."; exit 1
  fi
  python3 "$REPO_ROOT/scripts/fetch-steam-wine.py"
  source "$REPO_ROOT/scripts/steam-runtime.sh"
  WINE="$STEAM_WINE_ROOT/bin/wine"
fi
"$WINE" --version

# ---------- 2. Verified wrapper and rendering components ----------
python3 "$REPO_ROOT/scripts/fetch-steam-support.py"
if [ -f "$WINEPREFIX/drive_c/Program Files (x86)/Steam/steam.exe" ]; then
  echo "Steam already installed."
  exit 0
fi

# ---------- 3. Prefix + Steam install ----------
echo "==> Initializing the prefix: $WINEPREFIX"
"$WINE" wineboot -u >/dev/null 2>&1 || true
"$STEAM_WINE_ROOT/bin/wineserver" -w 2>/dev/null || true

SETUP="$WORK/SteamSetup.exe"
[ -f "$SETUP" ] || { curl -fL "https://cdn.cloudflare.steamstatic.com/client/installer/SteamSetup.exe" -o "$SETUP.part" && mv -f "$SETUP.part" "$SETUP"; }
echo "==> Installing Steam silently"
"$WINE" "$SETUP" /S >/dev/null 2>&1 || true
"$STEAM_WINE_ROOT/bin/wineserver" -w 2>/dev/null || true
[ -f "$WINEPREFIX/drive_c/Program Files (x86)/Steam/steam.exe" ] || { echo "Install failed"; exit 1; }

# ---------- 4. Finish ----------
cp -f /etc/ssl/cert.pem "$WINEPREFIX/drive_c/windows/cacert.pem" 2>/dev/null || true
echo "==> Done. Run: scripts/play.sh steam  (play.sh deploys the wrapper automatically)"
echo "    If typed text shows up as ??, switch the macOS input source to English (ABC)."
