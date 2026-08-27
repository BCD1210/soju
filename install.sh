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

[ "$(uname -m)" = "arm64" ] || { echo "Apple Silicon Mac 전용입니다."; exit 1; }
/usr/bin/pgrep -q oahd || { echo "Rosetta 2가 필요합니다: softwareupdate --install-rosetta"; exit 1; }

mkdir -p "$BASE"

# ---------- 1. 엔진 ----------
if [ -x "$ENGINE/bin/wine" ]; then
  say "[1/4] 엔진이 이미 있습니다 — 건너뜀"
else
  say "[1/4] 프리빌드 엔진 다운로드 (~수백MB, GPL — 소스: CodeWeavers 공개 Wine)"
  URL=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
        | grep -o '"browser_download_url": *"[^"]*soju-engine[^"]*"' | head -1 | grep -o 'https[^"]*')
  [ -n "$URL" ] || { echo "릴리스 자산을 찾지 못했습니다."; exit 1; }
  curl -fL "$URL" -o "$BASE/engine.tar.xz"
  mkdir -p "$ENGINE"
  tar -xJf "$BASE/engine.tar.xz" -C "$ENGINE"
  rm -f "$BASE/engine.tar.xz"
  DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib" "$ENGINE/bin/wine" --version >/dev/null \
    && echo "엔진 확인: $(DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib" "$ENGINE/bin/wine" --version)"
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
  say "[2/4] Apple GPTK 이미 설치됨 — 건너뜀"
else
  say "[2/4] Apple Game Porting Toolkit 필요 (무료, 1회)"
  if SRC=$(find_gptk); then
    echo "발견: $SRC — 자동 설치"
    install_gptk "$SRC"
  else
    cat <<'EOT'
  게임 실행에 애플의 파일 하나(libd3dshared)가 필요합니다. 애플이 재배포를 금지해서
  직접 받아야 합니다 (무료 Apple ID면 됩니다):
    1) https://developer.apple.com/games/game-porting-toolkit/ 접속
    2) "Game Porting Toolkit" dmg 다운로드 후 더블클릭(마운트)
    3) 이 창으로 돌아와 Enter
EOT
    while true; do
      read -r -p "  준비되면 Enter (건너뛰려면 s): " ans < "$TTY" || ans=s
      [ "$ans" = "s" ] && { echo "  건너뜀 — 나중에 scripts/get-gptk.sh 실행"; break; }
      if SRC=$(find_gptk); then install_gptk "$SRC"; echo "  설치됨"; break; fi
      echo "  아직 못 찾음 — dmg가 마운트됐는지 확인하세요"
    done
  fi
fi

# ---------- 3. 보틀 + Battle.net ----------
export WINEPREFIX="$BOTTLE" WINEDEBUG=fixme-all WINEMSYNC=1 ROSETTA_ADVERTISE_AVX=1
export CX_APPLEGPTK_LIBD3DSHARED_PATH="$ENGINE/lib/external/libd3dshared.dylib"
export DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib"
if [ -f "$BOTTLE/drive_c/Program Files (x86)/Battle.net/Battle.net.exe" ]; then
  say "[3/4] Battle.net 이미 설치됨 — 건너뜀"
else
  say "[3/4] 보틀 생성 + Battle.net 자동 설치 (5~10분)"
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
    || { echo "설치 실패 — $BOTTLE/drive_c/ProgramData/Battle.net/Setup 로그 확인"; exit 1; }
  "$ENGINE/bin/wineserver" -k 2>/dev/null || true
  echo "Battle.net 설치 완료"
fi

# ---------- 4. 앱 아이콘 ----------
say "[4/4] Battle.net.app 생성"
APP="$HOME/Applications/Battle.net.app"
mkdir -p "$APP/Contents/MacOS"
cat > "$APP/Contents/MacOS/launcher" <<EOF
#!/bin/bash
export WINEPREFIX="$BOTTLE" WINEDEBUG=fixme-all WINEMSYNC=1 ROSETTA_ADVERTISE_AVX=1
export CX_APPLEGPTK_LIBD3DSHARED_PATH="$ENGINE/lib/external/libd3dshared.dylib"
export DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib"
BN="\$WINEPREFIX/drive_c/Program Files (x86)/Battle.net"
for v in "\$BN"/Battle.net.[0-9]*; do [ -d "\$v" ] && cp -f "\$BN/Battle.net.exe" "\$v/Battle.net.exe" 2>/dev/null; done
exec "$ENGINE/bin/wine" "C:\\\\Program Files (x86)\\\\Battle.net\\\\Battle.net Launcher.exe" --disable-gpu-compositing
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

say "완료! 🍶  ~/Applications/Battle.net.app 을 더블클릭 → 로그인 → 게임 설치·플레이"
GPTK_OK || echo "※ GPTK를 건너뛰었습니다 — 게임 실행 전 scripts/get-gptk.sh 를 꼭 실행하세요."
