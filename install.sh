#!/usr/bin/env bash
# Soju one-line installer: Battle.net, Steam, Epic and GOG launchers on Apple Silicon, no CrossOver.
#   curl -fsSL soju.snack-wrap.com/install.sh | bash
#   curl -fsSL https://soju.snack-wrap.com/install.sh | SOJU_PLATFORMS=battlenet,epic,gog bash     (non-interactive selection)
#
# What it does:
#   1. Asks which launchers you want before downloading anything
#   2. Installs only their dependencies (Steam skips the CX engine and Apple GPTK)
#   3. Installs each official client into its own bottle
#   4. Creates a double-clickable app in ~/Applications for each launcher
set -euo pipefail

REPO="BCD1210/soju"
BASE="${SOJU_BASE:-$HOME/.battlenet-macos}"
export SOJU_BASE="$BASE"
ENGINE="${ENGINE:-$BASE/cx26-engine}"
BOTTLE="$BASE/bottle"
TTY=/dev/tty

say(){ printf '\n\033[1m%s\033[0m\n' "$*"; }

# ---------- 1. Choose launchers before any download ----------
ALL="battlenet steam epic gog"
if [ -n "${SOJU_PLATFORMS:-}" ]; then
  PLATFORMS="$(echo "$SOJU_PLATFORMS" | tr ',' ' ')"
else
  say "[1/4] Which launchers do you want? (numbers separated by spaces, Enter = Battle.net only)"
  cat <<'EOT'
  1) Battle.net           (Diablo II: Resurrected etc.)
  2) Steam                (Windows client on private Wine 11, D3D11 games via DXMT)
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
      *) echo "Unknown choice: $c"; exit 64 ;;
    esac
  done
fi
for p in $PLATFORMS; do case " $ALL " in *" $p "*) ;; *) echo "Unknown platform: $p"; exit 1;; esac; done
[ -n "${PLATFORMS// /}" ] || { echo "Nothing selected."; exit 64; }
echo "  Installing:$PLATFORMS"

NEEDS_CX=0; NEEDS_STEAM=0
for p in $PLATFORMS; do
  case "$p" in steam) NEEDS_STEAM=1 ;; *) NEEDS_CX=1 ;; esac
done
if [ "${1:-}" = "--plan" ]; then
  printf 'platforms=%s\ncx_engine=%s\napple_gptk=%s\nsteam_runtime=%s\n' "$PLATFORMS" "$NEEDS_CX" "$NEEDS_CX" "$NEEDS_STEAM"
  exit 0
fi

[ "$(uname -m)" = "arm64" ] || { echo "This installer is for Apple Silicon Macs only."; exit 1; }
/usr/bin/pgrep -q oahd || { echo "Rosetta 2 is required: softwareupdate --install-rosetta"; exit 1; }

mkdir -p "$BASE"

# ---------- Apple GPTK helpers ----------
# Installed means all three parts: the Metal side, and Apple's PE shims in
# place of Wine's d3d11 (the shim carries Apple's build path string).
GPTK_OK(){
  local f
  [ -f "$ENGINE/lib/external/libd3dshared.dylib" ] || return 1
  for f in d3d11 d3d12 dxgi; do
    grep -q "D3DMetalDLLsBase" "$ENGINE/lib/wine/x86_64-windows/$f.dll" 2>/dev/null || return 1
    [ -L "$ENGINE/lib/wine/x86_64-unix/$f.so" ] || return 1
  done
}
# Copy an installed GPTK payload from one engine to another (all three parts).
carry_gptk(){   # from, to
  local from="$1" to="$2" f b
  [ -f "$from/lib/external/libd3dshared.dylib" ] || return 0
  mkdir -p "$to/lib/external"
  cp -Rf "$from/lib/external/." "$to/lib/external/"
  for f in "$from"/lib/wine/x86_64-unix/*.so; do
    [ -L "$f" ] || continue
    b=$(basename "$f" .so)
    # only Apple's shims travel; a Wine DLL next to a symlink is an engine set
    # up by an older script, and must not be mistaken for one
    if grep -q "D3DMetalDLLsBase" "$from/lib/wine/x86_64-windows/$b.dll" 2>/dev/null; then
      cp -f "$from/lib/wine/x86_64-windows/$b.dll" "$to/lib/wine/x86_64-windows/$b.dll"
      ln -sf ../../external/libd3dshared.dylib "$to/lib/wine/x86_64-unix/$b.so"
    fi
  done
}
# ---------- 2. Engine (Battle.net / Epic / GOG only) ----------
if [ "$NEEDS_CX" = 1 ]; then
# bin/wine alone can be left behind by an interrupted extract; only an engine
# that actually starts counts as present.
if [ -x "$ENGINE/bin/wine" ] && DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib" "$ENGINE/bin/wine" --version >/dev/null 2>&1; then
  say "[2/4] Engine already present - skipping"
else
  say "[2/4] Downloading prebuilt engine (~350MB, GPL - built from CodeWeavers' published Wine sources)"
  # The engine ships on its own "engine-*" release, which is not necessarily the
  # newest release: scan all releases and take the first (newest) engine asset.
  URL=$(curl -fsSL "https://api.github.com/repos/$REPO/releases?per_page=30" \
        | grep -o '"browser_download_url": *"[^"]*wine-engine[^"]*"' | head -1 | grep -o 'https[^"]*')
  [ -n "$URL" ] || { echo "Could not find the engine release asset."; exit 1; }
  # Download to a .part file and extract into a staging directory, so an
  # interrupted run leaves nothing that a re-run would mistake for done.
  curl -fL "$URL" -o "$BASE/engine.tar.xz.part"
  mv -f "$BASE/engine.tar.xz.part" "$BASE/engine.tar.xz"
  rm -rf "$ENGINE.new"; mkdir -p "$ENGINE.new"
  tar -xJf "$BASE/engine.tar.xz" -C "$ENGINE.new"
  rm -f "$BASE/engine.tar.xz"
  TAG=$(echo "$URL" | sed -n 's#.*/download/\([^/]*\)/.*#\1#p')
  printf '%s\n' "${TAG:-unknown}" > "$ENGINE.new/.soju-engine-release"
  if [ -d "$ENGINE" ]; then
    # A broken engine from an earlier run; keep any GPTK payload in it.
    carry_gptk "$ENGINE" "$ENGINE.new"
    rm -rf "$ENGINE"
  fi
  mv "$ENGINE.new" "$ENGINE"
  v=$(DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib" "$ENGINE/bin/wine" --version 2>&1) \
    || { echo "The engine does not start on this Mac:"; echo "$v"; echo "Please report this with the output of: sw_vers; uname -m"; exit 1; }
  echo "Engine OK: $v"
fi

fi # NEEDS_CX

# ---------- Soju scripts ----------
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

# ---------- Apple GPTK (Battle.net / Epic / GOG only) ----------
if [ "$NEEDS_CX" = 1 ]; then
# scripts/get-gptk.sh finds the payload (mounted toolkit dmg, or CrossOver),
# verifies every file (Apple's signature on the Mach-O parts, known Apple
# builds for the PE shims) and installs it. See the notes in that script.
GPTK="$SOJU_DIR/scripts/get-gptk.sh"
if GPTK_OK; then
  say "[2/4] Apple GPTK already installed - skipping"
else
  say "[2/4] Apple Game Porting Toolkit needed (free, one time)"
  if SRC=$(ENGINE="$ENGINE" bash "$GPTK" --find); then
    echo "Found: $SRC - installing"
    ENGINE="$ENGINE" bash "$GPTK" "$SRC" || echo "  GPTK not installed (see above); the last lines of this installer say how to add it later"
  else
    cat <<'EOT'
  Running games needs one Apple file (libd3dshared). Apple forbids redistributing
  it, so you have to download it yourself (a free Apple ID is enough):
    1) Open https://developer.apple.com/games/game-porting-toolkit/
    2) Download the "evaluation environment for Windows games" dmg (that is
       the Game Porting Toolkit download) and double-click it to mount it
    3) Come back to this window and press Enter
EOT
    while true; do
      [ "${SOJU_NONINTERACTIVE:-0}" != 1 ] || { echo "  GPTK not mounted. Use Add GPTK in Soju, then install again."; break; }
      read -r -p "  Press Enter when the dmg is mounted, or paste the path of the mounted volume (s to skip): " ans < "$TTY" || ans=s
      [ "$ans" = "s" ] && { echo "  Skipped - the last lines of this installer say how to add it later"; break; }
      ans=${ans//\\ / }; ans=${ans%"${ans##*[! ]}"}   # a path dragged into Terminal comes escaped
      if SRC=$(ENGINE="$ENGINE" bash "$GPTK" --find "$ans"); then
        ENGINE="$ENGINE" bash "$GPTK" "$SRC" || echo "  GPTK not installed (see above); the last lines of this installer say how to add it later"
        break
      fi
      echo "  Not found yet. Disk images mounted right now:"
      bash "$GPTK" --list-images | sed 's/^/    /'
      echo "  If the toolkit is mounted under another name, paste its path (you can drag the volume from Finder into this window)."
    done
  fi
fi

fi # NEEDS_CX

# ---------- 3. Launchers ----------

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
      say "Steam: checking runtime and client (existing games are preserved)"
      WINEPREFIX="$BASE/steam-bottle" bash "$SOJU_DIR/scripts/create-steam-bottle.sh"
      WINEPREFIX="$BASE/steam-bottle" bash "$SOJU_DIR/scripts/setup-steam-games.sh" ;;
    epic)
      if [ -f "$BASE/epic-bottle/drive_c/Program Files/Epic Games/Launcher/Portal/Binaries/Win64/EpicGamesLauncher.exe" ]; then
        say "Epic Games Launcher already installed - skipping"
      else
        say "Epic Games Launcher: creating the bottle + running Epic's installer"
        WINEPREFIX="$BASE/epic-bottle" bash "$SOJU_DIR/scripts/create-epic-bottle.sh"
      fi ;;
    gog)
      if [ -f "$BASE/gog-bottle/drive_c/Program Files/GOG Galaxy/GalaxyClient.exe" ]; then
        say "GOG GALAXY already installed - skipping"
      else
        say "GOG GALAXY: creating the bottle + running GOG's installer"
        WINEPREFIX="$BASE/gog-bottle" bash "$SOJU_DIR/scripts/create-gog-bottle.sh"
      fi ;;
  esac
  # The client EXE does not prove its helper build completed.
  case "$p" in
    epic|gog) bash "$SOJU_DIR/scripts/ensure-launcher-helper.sh" "$p" ;;
  esac
done

# ---------- 4. App bundles ----------
say "[4/4] Creating apps in ~/Applications"
make_app(){   # name, play.sh mode, bundle id
  local APP="$HOME/Applications/$1.app"
  mkdir -p "$APP/Contents/MacOS"
  {
    printf '#!/bin/bash\n'
    printf 'export ENGINE=%q\nexport SOJU_BASE=%q\n' "$ENGINE" "$BASE"
    printf 'exec /bin/bash %q %q\n' "$SOJU_DIR/scripts/play.sh" "$2"
  } > "$APP/Contents/MacOS/launcher"
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
[ "$NEEDS_CX" = 0 ] || GPTK_OK || echo "NOTE: GPTK was skipped - before launching a game, mount the GPTK dmg and run:  $SOJU_DIR/scripts/soju gptk"
echo
echo "Something broke?  Run 'soju doctor' and paste its output in an issue: https://github.com/BCD1210/soju/issues"
echo "Worked for you?   A star helps other Mac gamers find this:           https://github.com/BCD1210/soju"
