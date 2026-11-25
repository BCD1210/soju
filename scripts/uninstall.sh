#!/usr/bin/env bash
# soju uninstall: remove what soju created, asking per category.
#   soju uninstall          interactive
#   soju uninstall --yes    remove everything without asking
#   soju uninstall launcher <battlenet|steam|epic|gog|ea|rockstar|ubisoft|xbox>
#                           remove one launcher, its games and its app; nothing else
set -euo pipefail

BASE="${SOJU_BASE:-$HOME/.battlenet-macos}"
ENGINE="${ENGINE:-$BASE/cx26-engine}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
source "$ROOT/scripts/steam-runtime.sh"
TTY=/dev/tty
REMOVED=0
YES=0; [ "${1:-}" = "--yes" ] && YES=1

ask(){
  [ "$YES" = 1 ] && return 0
  read -r -p "$1 [y/N]: " a < "$TTY" || a=n
  case "$a" in y|Y|yes) return 0;; *) return 1;; esac
}

# Does this launcher script belong to this install? install.sh writes the engine path with
# printf %q (no quotes); older ones wrote it in double quotes.
names_engine(){
  grep -qxF "$(printf 'export ENGINE=%q' "$ENGINE")" "$1" || grep -qF "ENGINE=\"$ENGINE\"" "$1"
}

[ "${1:-}" = launcher ] || echo "soju uninstall: this removes things soju created. Nothing is touched without a yes."

# Stop only after this bottle's removal (or engine removal) was accepted.
stop_bottle() {
  local b="$1" ws="$ENGINE/bin/wineserver"
  [ -d "$BASE/$b" ] || return 0
  case "$b" in ea-bottle|rockstar-bottle|ubisoft-bottle)
    [ -x "$BASE/cx26-engine-launchers/bin/wineserver" ] && ws="$BASE/cx26-engine-launchers/bin/wineserver" ;;
  esac
  if [ "$b" = steam-bottle ]; then
    # This resolves private, legacy and explicit runtimes exactly as play does.
    # A failed stop/wait aborts uninstall before any bottle files are removed.
    python3 "$ROOT/scripts/steam-session.py" stop "$STEAM_WINE_ROOT" "$BASE/$b"
    return
  fi
  WINEPREFIX="$BASE/$b" DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib" "$ws" -k 2>/dev/null || true
  WINEPREFIX="$BASE/$b" DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib" "$ws" -w 2>/dev/null || true
}

# One launcher (the app's Remove button): its bottle(s), with the games inside, and its
# ~/Applications shortcut. Engines, runtimes and the other launchers stay.
if [ "${1:-}" = launcher ]; then
  case "${2:-}" in
    battlenet) bottles=(bottle); app="Battle.net" ;;
    steam) bottles=(steam-bottle steam-gptk-bottle); app="Steam (Windows)" ;;
    epic) bottles=(epic-bottle); app="Epic Games Launcher" ;;
    gog) bottles=(gog-bottle); app="GOG GALAXY" ;;
    ea) bottles=(ea-bottle); app="EA app" ;;
    rockstar) bottles=(rockstar-bottle); app="Rockstar Games Launcher" ;;
    ubisoft) bottles=(ubisoft-bottle); app="Ubisoft Connect" ;;
    xbox) bottles=(xbox-bottle); app="" ;;
    *) echo "Choose one of the launchers."; exit 64 ;;
  esac
  found=0
  for b in "${bottles[@]}"; do
    d="$BASE/$b"
    [ -d "$d" ] || continue
    found=1
    echo "Removing $b ($(du -sh "$d" 2>/dev/null | cut -f1), with the games installed in it)…"
    if [ "$b" = xbox-bottle ]; then
      bash "$ROOT/scripts/xbox.sh" kill >/dev/null 2>&1 || true
      sh "$ROOT/scripts/xbox-wine.sh" --soju-stop-prefix 2>/dev/null || true
    fi
    stop_bottle "$b"
    # Games moved to another drive are links here: rm removes the link, not the files on the drive.
    rm -rf "$d"
  done
  [ "${2}" != steam ] || rm -f "$BASE/steam-gptk"
  l="$HOME/Applications/$app.app/Contents/MacOS/launcher"
  if [ -n "$app" ] && [ -f "$l" ] && grep -q "scripts/play.sh" "$l" && names_engine "$l"; then
    rm -rf "$HOME/Applications/$app.app"; found=1
  fi
  [ "$found" = 1 ] || { echo "Nothing to remove for ${2}."; exit 0; }
  echo "Removed. Your account and purchases stay with the store; install it again from Stores any time."
  exit 0
fi

# 2. App bundles.
# Only bundles install.sh wrote for this install (their launcher script names
# this engine), so a custom SOJU_BASE never removes another install's apps.
apps=()
for a in "Battle.net" "Steam (Windows)" "Epic Games Launcher" "GOG GALAXY" "EA app" "Rockstar Games Launcher" "Ubisoft Connect"; do
  l="$HOME/Applications/$a.app/Contents/MacOS/launcher"
  [ -f "$l" ] && grep -q "scripts/play.sh" "$l" && names_engine "$l" && apps+=("$HOME/Applications/$a.app")
done
if [ "${#apps[@]}" -gt 0 ]; then
  printf 'Apps: %s\n' "${apps[@]}"
  if ask "Remove these apps?"; then rm -rf "${apps[@]}"; REMOVED=1; echo "  removed"; fi
fi

# 3. Bottles (this is where installed games live; can be tens of GB).
for b in bottle steam-bottle epic-bottle gog-bottle ea-bottle rockstar-bottle ubisoft-bottle xbox-bottle; do
  d="$BASE/$b"
  [ -d "$d" ] || continue
  sz=$(du -sh "$d" 2>/dev/null | cut -f1)
  if ask "Remove bottle $b ($sz, includes installed games)?"; then
    if [ "$b" = xbox-bottle ]; then
      bash "$ROOT/scripts/xbox.sh" kill >/dev/null 2>&1 || true
      sh "$ROOT/scripts/xbox-wine.sh" --soju-stop-prefix 2>/dev/null || true
    fi
    stop_bottle "$b"
    rm -rf "$d"; REMOVED=1; echo "  removed"
  fi
done

# Steam engines have their own category, including rollback copies.
runtimes=()
for d in steam-runtime steam-runtime.previous steam-wine steam-wine.previous; do
  [ -e "$BASE/$d" ] && runtimes+=("$BASE/$d")
done
if [ "${#runtimes[@]}" -gt 0 ]; then
  printf 'Steam runtimes: %s\n' "${runtimes[@]}"
  if ask "Remove these Steam runtimes (can be downloaded again)?"; then
    stop_bottle steam-bottle
    python3 "$ROOT/scripts/steam-session.py" idle "$STEAM_WINE_ROOT"
    rm -rf "${runtimes[@]}"; REMOVED=1; echo "  removed"
  fi
fi

# The Xodus backend build (re-buildable from source with soju xbox-install).
xodus=()
for d in xodus xodus.previous .xodus-stage xbox-runtime xbox-runtime.previous run; do
  [ -e "$BASE/$d" ] && xodus+=("$BASE/$d")
done
if [ "${#xodus[@]}" -gt 0 ]; then
  printf 'Xbox PC backend and game runtime: %s\n' "${xodus[@]}"
  # Offer the Microsoft sign-in too while the backend that can remove it still exists.
  if [ -x "$BASE/xodus/bin/xodus-cli" ] && security find-generic-password -s "Xodus Service" >/dev/null 2>&1 &&
     ask "Sign out of Microsoft (removes the Xbox sign-in from the Keychain)?"; then
    XODUS_LOG=error "$BASE/xodus/bin/xodus-cli" logout --device >/dev/null 2>&1 && echo "  signed out" || echo "  sign-out failed; remove \"Xodus Service\" in Keychain Access"
  fi
  if ask "Remove the Xodus backend and Xbox game runtime (can be downloaded and built again)?"; then
    xodus_bin="$(printf '%s' "$BASE/xodus/bin/xodus-" | sed 's/[][\.*^$()+?{}|]/\\&/g')"
    pkill -f "^${xodus_bin}(cli|service)( |$)" 2>/dev/null || true
    rm -rf "${xodus[@]}"; REMOVED=1; echo "  removed"
  fi
fi

# 4. Engine (+ the GPTK payload inside it).
if [ -d "$ENGINE" ]; then
  sz=$(du -sh "$ENGINE" 2>/dev/null | cut -f1)
  if ask "Remove the engine ($sz, includes the GPTK payload; re-downloadable with soju install)?"; then
    for b in bottle epic-bottle gog-bottle ea-bottle rockstar-bottle ubisoft-bottle; do stop_bottle "$b"; done
    rm -rf "$ENGINE" "$BASE/cx26-engine-launchers"; REMOVED=1; echo "  removed"
  fi
fi

# 5. Caches and support files: downloaded installers and sources (build/),
#    compiled helpers (*-support/), logs, and leftovers of an interrupted update.
extras=()
for d in build steam-support gstreamer epic-support gog-support launcher-support logs cx26-engine.old cx26-engine.new cx26-engine-launchers.new soju.old; do
  [ -e "$BASE/$d" ] && extras+=("$BASE/$d")
done
for d in "$BASE"/reaper-*.log "$BASE"/*.tar.xz "$BASE"/*.part; do
  [ -f "$d" ] && extras+=("$d")
done
if [ "${#extras[@]}" -gt 0 ]; then
  sz=$(du -shc "${extras[@]}" 2>/dev/null | tail -1 | cut -f1)
  printf 'Caches and support files (%s):\n' "$sz"; printf '  %s\n' "${extras[@]}"
  if ask "Remove these?"; then rm -rf "${extras[@]}"; REMOVED=1; echo "  removed"; fi
fi

# 6. The scripts copy made by the one-line installer.
if [ -d "$BASE/soju" ]; then
  if ask "Remove the scripts copy at $BASE/soju?"; then rm -rf "$BASE/soju"; REMOVED=1; echo "  removed"; fi
fi
[ "$REMOVED" = 0 ] || { rmdir "$BASE" 2>/dev/null && echo "Removed empty $BASE" || true; }

cat <<'EOT'

Not removed by this script (they are Homebrew's):
  brew uninstall soju                    the soju CLI itself, if installed via brew
  brew uninstall --cask wine-stable      legacy Wine, only if separately installed
  brew uninstall mingw-w64               the wrapper build dependency
  brew uninstall protobuf rustup         Xodus build dependencies, if Soju installed them
If you kept the Microsoft sign-in, it stays in your Keychain as "Xodus Service"
(remove it in Keychain Access).
Xbox games downloaded to an external drive stay on that drive (delete their
folders in Finder if you no longer need them).
EOT
echo "Done."
