#!/usr/bin/env bash
# Install Steam (Windows build): for Steam games such as the Steam edition of
# D2R (Infernal Edition).
#
# Important: unlike Battle.net, Steam runs on Homebrew wine-stable, not the CX
# source engine. CX-derived builds cannot render Steam's CEF UI (black screen);
# the verified free combination is wine-stable 11 + the steamwebhelper wrapper.
# Wrapper source: github.com/notpop/steam-on-m1-wine (MIT): source and license
# vendored in third_party/.
# Verified 2026-08-27 on M4 Pro / macOS 26.5: login screen renders.
set -euo pipefail

export WINEPREFIX="${WINEPREFIX:-$HOME/.battlenet-macos/steam-bottle}"
SUPPORT="$HOME/.battlenet-macos/steam-support"
WORK="${WORK:-$HOME/.battlenet-macos/build}"; mkdir -p "$WORK" "$SUPPORT"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WINESTABLE_APP="/Applications/Wine Stable.app"
WINE="$WINESTABLE_APP/Contents/Resources/wine/bin/wine"

# ---------- 1. wine-stable ----------
if [ ! -x "$WINE" ]; then
  echo "==> Installing Homebrew wine-stable (free, ~500MB)"
  # The gstreamer-runtime cask dependency demands sudo, so skip it (Steam's UI doesn't need it)
  brew install --cask --skip-cask-deps wine-stable
  echo "==> Removing the Gatekeeper quarantine flag (takes a few minutes)"
  xattr -dr com.apple.quarantine "$WINESTABLE_APP" 2>/dev/null || true
fi
"$WINE" --version

# ---------- 2. webhelper wrapper ----------
# Forces Steam's CEF into --disable-gpu --single-process to avoid the black screen.
if [ ! -f "$SUPPORT/steamwebhelper-wrapper.exe" ]; then
  echo "==> Building the wrapper (mingw-w64)"
  command -v x86_64-w64-mingw32-gcc >/dev/null || brew install mingw-w64
  x86_64-w64-mingw32-gcc -O2 -Wall -municode -DUNICODE -D_UNICODE \
    -o "$SUPPORT/steamwebhelper-wrapper.exe" \
    "$REPO_ROOT/third_party/steam-on-m1-wine/steamwebhelper-wrapper.c" \
    -static -lshell32 -mwindows
fi

# ---------- 3. Prefix + Steam install ----------
echo "==> Initializing the prefix: $WINEPREFIX"
"$WINE" wineboot -u >/dev/null 2>&1 || true
"$WINESTABLE_APP/Contents/Resources/wine/bin/wineserver" -w 2>/dev/null || true

SETUP="$WORK/SteamSetup.exe"
[ -f "$SETUP" ] || { curl -fL "https://cdn.cloudflare.steamstatic.com/client/installer/SteamSetup.exe" -o "$SETUP.part" && mv -f "$SETUP.part" "$SETUP"; }
echo "==> Installing Steam silently"
"$WINE" "$SETUP" /S >/dev/null 2>&1 || true
"$WINESTABLE_APP/Contents/Resources/wine/bin/wineserver" -w 2>/dev/null || true
[ -f "$WINEPREFIX/drive_c/Program Files (x86)/Steam/steam.exe" ] || { echo "Install failed"; exit 1; }

# ---------- 4. Finish ----------
cp -f /etc/ssl/cert.pem "$WINEPREFIX/drive_c/windows/cacert.pem" 2>/dev/null || true
echo "==> Done. Run: scripts/play.sh steam  (play.sh deploys the wrapper automatically)"
echo "    If typed text shows up as ??, switch the macOS input source to English (ABC)."
