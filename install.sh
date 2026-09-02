#!/usr/bin/env bash
# Soju one-line installer: Battle.net, Steam, Epic and GOG launchers on Apple Silicon, no CrossOver.
#   curl -fsSL soju.snack-wrap.com/install.sh | bash
#   SOJU_PLATFORMS=battlenet,epic,gog curl ... | bash     (non-interactive selection)
#
# What it does:
#   1. Downloads the prebuilt GPL Wine engine (built from CodeWeavers' published sources)
#   2. Asks you to download Apple's Game Porting Toolkit once (Apple forbids redistribution)
#   3. Asks which launchers you want (any combination) and installs each one with
#      its official installer into its own bottle
#   4. Creates a double-clickable app in ~/Applications for each launcher
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
  # The engine ships on its own "engine-*" release, which is not necessarily the
  # newest release: scan all releases and take the first (newest) engine asset.
  URL=$(curl -fsSL "https://api.github.com/repos/$REPO/releases?per_page=30" \
        | grep -o '"browser_download_url": *"[^"]*soju-engine[^"]*"' | head -1 | grep -o 'https[^"]*')
  [ -n "$URL" ] || { echo "Could not find the engine release asset."; exit 1; }
  curl -fL "$URL" -o "$BASE/engine.tar.xz"
  mkdir -p "$ENGINE"
  tar -xJf "$BASE/engine.tar.xz" -C "$ENGINE"
  rm -f "$BASE/engine.tar.xz"
  TAG=$(echo "$URL" | sed -n 's#.*/download/\([^/]*\)/.*#\1#p')
  [ -n "$TAG" ] && printf '%s\n' "$TAG" > "$ENGINE/.soju-engine-release"
  v=$(DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib" "$ENGINE/bin/wine" --version 2>&1) \
    || { echo "The engine does not start on this Mac:"; echo "$v"; echo "Please report this with the output of: sw_vers; uname -m"; exit 1; }
  echo "Engine OK: $v"
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
      [ "$ans" = "s" ] && { echo "  Skipped - the last lines of this installer say how to add it later"; break; }
      if SRC=$(find_gptk); then install_gptk "$SRC"; echo "  Installed"; break; fi
      echo "  Not found yet - make sure the dmg is mounted"
    done
  fi
fi

# ---------- 3. Launchers ----------
# The bottle scripts live in the repo; fetch a copy next to the engine so the
# one-line installer can use them (a checkout running install.sh uses itself).
SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" 2>/dev/null && pwd || true)"
if [ -n "$SELF_DIR" ] && [ -f "$SELF_DIR/scripts/play.sh" ]; then
  SOJU_DIR="$SELF_DIR"
  # Homebrew: the Cellar path carries the version and goes away on `brew
  # upgrade`, which would leave every app bundle pointing at nothing. Bake the
  # stable opt/ path into the bundles instead.
  case "$SOJU_DIR" in
    */Cellar/soju/*/libexec) SOJU_DIR="$(brew --prefix 2>/dev/null || echo /opt/homebrew)/opt/soju/libexec" ;;
  esac
else
  SOJU_DIR="$BASE/soju"
  say "Fetching the Soju scripts into $SOJU_DIR"
  rm -rf "$SOJU_DIR.tmp"; mkdir -p "$SOJU_DIR.tmp"
  curl -fsSL "https://github.com/$REPO/archive/refs/heads/main.tar.gz" | tar -xz -C "$SOJU_DIR.tmp" --strip-components=1
  rm -rf "$SOJU_DIR"; mv "$SOJU_DIR.tmp" "$SOJU_DIR"
fi
chmod +x "$SOJU_DIR"/scripts/*.sh "$SOJU_DIR"/scripts/soju 2>/dev/null || true

ALL="battlenet steam epic gog"
if [ -n "${SOJU_PLATFORMS:-}" ]; then
  PLATFORMS="$(echo "$SOJU_PLATFORMS" | tr ',' ' ')"
else
  say "[3/4] Which launchers do you want? (numbers separated by spaces, Enter = Battle.net only)"
  cat <<'EOT'
  1) Battle.net           (Diablo II: Resurrected etc.)
  2) Steam                (Windows client on Homebrew wine-stable, D3D11 games via DXMT)
  3) Epic Games Launcher
  4) GOG GALAXY
EOT
  read -r -p "  Your choice [1]: " choice < "$TTY" || choice=1
  [ -n "$choice" ] || choice=1
  PLATFORMS=""
  for c in $choice; do
    case "$c" in
      1|battlenet) PLATFORMS="$PLATFORMS battlenet" ;;
      2|steam)     PLATFORMS="$PLATFORMS steam" ;;
      3|epic)      PLATFORMS="$PLATFORMS epic" ;;
      4|gog)       PLATFORMS="$PLATFORMS gog" ;;
      *) echo "  Ignoring unknown choice: $c" ;;
    esac
  done
fi
for p in $PLATFORMS; do case " $ALL " in *" $p "*) ;; *) echo "Unknown platform: $p"; exit 1;; esac; done
[ -n "$PLATFORMS" ] || { echo "Nothing selected."; exit 1; }
echo "  Installing:$PLATFORMS"

export ENGINE
for p in $PLATFORMS; do
  case "$p" in
    battlenet)
      if [ -f "$BOTTLE/drive_c/Program Files (x86)/Battle.net/Battle.net.exe" ]; then
        say "Battle.net already installed - skipping"
      else
        say "Battle.net: creating the bottle + running Blizzard's installer (5-10 min)"
        WINEPREFIX="$BOTTLE" bash "$SOJU_DIR/scripts/create-bottle.sh"
      fi ;;
    steam)
      if [ -f "$BASE/steam-bottle/drive_c/Program Files (x86)/Steam/steam.exe" ]; then
        say "Steam already installed - skipping"
      else
        say "Steam: Homebrew wine-stable + Steam installer"
        bash "$SOJU_DIR/scripts/create-steam-bottle.sh"
      fi ;;
    epic)
      if [ -f "$BASE/epic-bottle/drive_c/Program Files/Epic Games/Launcher/Portal/Binaries/Win64/EpicGamesLauncher.exe" ]; then
        say "Epic Games Launcher already installed - skipping"
      else
        say "Epic Games Launcher: creating the bottle + running Epic's installer"
        bash "$SOJU_DIR/scripts/create-epic-bottle.sh"
      fi ;;
    gog)
      if [ -f "$BASE/gog-bottle/drive_c/Program Files/GOG Galaxy/GalaxyClient.exe" ]; then
        say "GOG GALAXY already installed - skipping"
      else
        say "GOG GALAXY: creating the bottle + running GOG's installer"
        bash "$SOJU_DIR/scripts/create-gog-bottle.sh"
      fi ;;
  esac
done

# ---------- 4. App bundles ----------
say "[4/4] Creating apps in ~/Applications"
make_app(){   # name, play.sh mode, bundle id
  local APP="$HOME/Applications/$1.app"
  mkdir -p "$APP/Contents/MacOS"
  cat > "$APP/Contents/MacOS/launcher" <<EOF
#!/bin/bash
export ENGINE="$ENGINE"
exec "$SOJU_DIR/scripts/play.sh" $2
EOF
  chmod +x "$APP/Contents/MacOS/launcher"
  cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>$1</string>
<key>CFBundleExecutable</key><string>launcher</string>
<key>CFBundleIdentifier</key><string>$3</string>
<key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>
EOF
  codesign -f -s - "$APP" 2>/dev/null || true
  echo "  ~/Applications/$1.app"
}
mkdir -p "$HOME/Applications"
for p in $PLATFORMS; do
  case "$p" in
    battlenet) make_app "Battle.net" battlenet app.soju.battlenet ;;
    steam)     make_app "Steam (Windows)" steam app.soju.steam ;;
    epic)      make_app "Epic Games Launcher" epic app.soju.epic ;;
    gog)       make_app "GOG GALAXY" gog app.soju.gog ;;
  esac
done

say "Done! 🍶  Double-click the app(s) above, log in, install and play."
echo "Command line: $SOJU_DIR/scripts/soju  (battlenet | d2r | kill | epic | epic-kill | gog | gog-kill | steam | steam-kill | doctor | update)"
GPTK_OK || echo "NOTE: GPTK was skipped - before launching a game, mount the GPTK dmg and run:  $SOJU_DIR/scripts/soju gptk"
