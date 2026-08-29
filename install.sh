#!/usr/bin/env bash
# Soju one-line installer — Battle.net + Diablo II: Resurrected on Apple Silicon, no CrossOver.
#   curl -fsSL https://raw.githubusercontent.com/BCD1210/soju/main/install.sh | bash
#
# What it does:
#   1. Downloads the prebuilt GPL Wine engine (built from CodeWeavers' published sources)
#   2. Asks you to download Apple's Game Porting Toolkit once (Apple forbids redistribution)
#   3. Creates a bottle and runs Blizzard's official Battle.net installer (automatic)
#   4. Creates ~/Applications/Battle.net.app
set -euo pipefail

REPO="BCD1210/soju"
BASE="$HOME/.battlenet-macos"
ENGINE="$BASE/cx26-engine"
BOTTLE="$BASE/bottle"
TTY=/dev/tty

say(){ printf '\n\033[1m%s\033[0m\n' "$*"; }

[ "$(uname -m)" = "arm64" ] || { echo "This installer is for Apple Silicon Macs only."; exit 1; }
/usr/bin/pgrep -q oahd || { echo "Rosetta 2 is required: softwareupdate --install-rosetta"; exit 1; }

mkdir -p "$BASE"

# ---------- 1. Engine ----------
if [ -x "$ENGINE/bin/wine" ]; then
  say "[1/4] Engine already present - skipping"
else
  say "[1/4] Downloading prebuilt engine (~350MB, GPL - built from CodeWeavers' published Wine sources)"
  URL=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
        | grep -o '"browser_download_url": *"[^"]*soju-engine[^"]*"' | head -1 | grep -o 'https[^"]*')
  [ -n "$URL" ] || { echo "Could not find the engine release asset."; exit 1; }
  curl -fL "$URL" -o "$BASE/engine.tar.xz"
  mkdir -p "$ENGINE"
  tar -xJf "$BASE/engine.tar.xz" -C "$ENGINE"
  rm -f "$BASE/engine.tar.xz"
  DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib" "$ENGINE/bin/wine" --version >/dev/null \
    && echo "Engine OK: $(DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib" "$ENGINE/bin/wine" --version)"
fi

# ---------- 2. Apple GPTK ----------
GPTK_OK(){ [ -f "$ENGINE/lib/external/libd3dshared.dylib" ]; }
find_gptk(){
  for r in /Volumes/Game* /Volumes/*orting* \
           "/Applications/CrossOver.app/Contents/SharedSupport/CrossOver/lib64/apple_gptk"; do
    [ -d "$r" ] || continue
    f=$(find "$r" -maxdepth 6 -name "libd3dshared.dylib" 2>/dev/null | head -1)
    [ -n "$f" ] && { dirname "$f"; return 0; }
  done
  return 1
}
install_gptk(){
  local SRC="$1"
  mkdir -p "$ENGINE/lib/external"
  cp -Rf "$SRC/D3DMetal.framework" "$ENGINE/lib/external/" 2>/dev/null || true
  cp -f  "$SRC/libd3dshared.dylib" "$ENGINE/lib/external/"
  ( cd "$ENGINE/lib/wine/x86_64-unix" \
    && for f in d3d10.so d3d11.so d3d12.so dxgi.so; do ln -sf ../../external/libd3dshared.dylib "$f"; done )
}
if GPTK_OK; then
  say "[2/4] Apple GPTK already installed - skipping"
else
  say "[2/4] Apple Game Porting Toolkit needed (free, one time)"
  if SRC=$(find_gptk); then
    echo "Found: $SRC - installing automatically"
    install_gptk "$SRC"
  else
    cat <<'EOT'
  Running games needs one Apple file (libd3dshared). Apple forbids redistributing
  it, so you have to download it yourself (a free Apple ID is enough):
    1) Open https://developer.apple.com/games/game-porting-toolkit/
    2) Download the "Game Porting Toolkit" dmg and double-click it (mount)
    3) Come back to this window and press Enter
EOT
    while true; do
      read -r -p "  Press Enter when ready (or s to skip): " ans < "$TTY" || ans=s
      [ "$ans" = "s" ] && { echo "  Skipped - run scripts/get-gptk.sh later"; break; }
      if SRC=$(find_gptk); then install_gptk "$SRC"; echo "  Installed"; break; fi
      echo "  Not found yet - make sure the dmg is mounted"
    done
  fi
fi

# ---------- 3. Bottle + Battle.net ----------
export WINEPREFIX="$BOTTLE" WINEDEBUG=fixme-all WINEMSYNC=1 ROSETTA_ADVERTISE_AVX=1 WINE_SIMULATE_WRITECOPY=1
export CX_APPLEGPTK_LIBD3DSHARED_PATH="$ENGINE/lib/external/libd3dshared.dylib"
export DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib"
if [ -f "$BOTTLE/drive_c/Program Files (x86)/Battle.net/Battle.net.exe" ]; then
  say "[3/4] Battle.net already installed - skipping"
else
  say "[3/4] Creating the bottle + installing Battle.net automatically (5-10 min)"
  "$ENGINE/bin/wine" wineboot -u >/dev/null 2>&1 || true
  "$ENGINE/bin/wineserver" -w
  W="$ENGINE/bin/wine"
  "$W" reg add 'HKCU\Software\Wine\WineDbg' /v ShowCrashDialog /t REG_DWORD /d 0 /f >/dev/null 2>&1
  for HIVE in 'HKLM\Software\Microsoft\Windows NT\CurrentVersion\AeDebug' \
              'HKLM\Software\Wow6432Node\Microsoft\Windows NT\CurrentVersion\AeDebug'; do
    "$W" reg add "$HIVE" /v Debugger /t REG_SZ /d 'C:\windows\system32\rundll32.exe kernel32.dll,Sleep' /f >/dev/null 2>&1
    "$W" reg add "$HIVE" /v Auto /t REG_SZ /d 1 /f >/dev/null 2>&1
  done
  "$ENGINE/bin/wineserver" -w
  SETUP="$BASE/Battle.net-Setup.exe"
  [ -f "$SETUP" ] || curl -fL "https://downloader.battle.net/download/getInstaller?os=win&installer=Battle.net-Setup.exe" -o "$SETUP"
  "$ENGINE/bin/wine" "$SETUP" --lang=enUS >/dev/null 2>&1 || true
  for i in $(seq 1 60); do
    [ -f "$BOTTLE/drive_c/Program Files (x86)/Battle.net/Battle.net.exe" ] && break; sleep 10
  done
  [ -f "$BOTTLE/drive_c/Program Files (x86)/Battle.net/Battle.net.exe" ] \
    || { echo "Install failed - check the logs in $BOTTLE/drive_c/ProgramData/Battle.net/Setup"; exit 1; }
  "$ENGINE/bin/wineserver" -k 2>/dev/null || true
  echo "Battle.net installed"
fi

# ---------- 4. App bundle ----------
say "[4/4] Creating Battle.net.app"
REAPER="$BASE/soju-reaper.sh"
[ -f "$REAPER" ] || curl -fsSL "https://raw.githubusercontent.com/$REPO/main/scripts/soju-reaper.sh" -o "$REAPER" 2>/dev/null || true
chmod +x "$REAPER" 2>/dev/null || true
APP="$HOME/Applications/Battle.net.app"
mkdir -p "$APP/Contents/MacOS"
cat > "$APP/Contents/MacOS/launcher" <<EOF
#!/bin/bash
export WINEPREFIX="$BOTTLE" WINEDEBUG=fixme-all WINEMSYNC=1 ROSETTA_ADVERTISE_AVX=1 WINE_SIMULATE_WRITECOPY=1
export CX_APPLEGPTK_LIBD3DSHARED_PATH="$ENGINE/lib/external/libd3dshared.dylib"
export DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib"
BN="\$WINEPREFIX/drive_c/Program Files (x86)/Battle.net"
for v in "\$BN"/Battle.net.[0-9]*; do [ -d "\$v" ] && cp -f "\$BN/Battle.net.exe" "\$v/Battle.net.exe" 2>/dev/null; done
# Reap zombie game processes so quitting the game really quits it
pgrep -f "soju-reaper.sh \$WINEPREFIX" >/dev/null 2>&1 || { [ -x "$REAPER" ] && ( "$REAPER" "\$WINEPREFIX" "$ENGINE/bin/wineserver" >/dev/null 2>&1 & ); }
# Battle.net.exe directly (not Launcher.exe): CrossOver's private compat DB injects these two
# switches; without them CEF's GPU process dies and the main window stays transparent.
exec "$ENGINE/bin/wine" "C:\\\\Program Files (x86)\\\\Battle.net\\\\Battle.net.exe" --disable-gpu-compositing --from-launcher --in-process-gpu --use-gl=swiftshader
EOF
chmod +x "$APP/Contents/MacOS/launcher"
cat > "$APP/Contents/Info.plist" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Battle.net</string>
<key>CFBundleExecutable</key><string>launcher</string>
<key>CFBundleIdentifier</key><string>app.soju.battlenet</string>
<key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>
EOF
codesign -f -s - "$APP" 2>/dev/null || true

say "Done! 🍶  Double-click ~/Applications/Battle.net.app, log in, install and play"
GPTK_OK || echo "NOTE: GPTK was skipped - run scripts/get-gptk.sh before launching a game."
