#!/usr/bin/env bash
# soju uninstall: remove what soju created, asking per category.
#   soju uninstall          interactive
#   soju uninstall --yes    remove everything without asking
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

echo "soju uninstall: this removes things soju created. Nothing is touched without a yes."

# Stop only after this bottle's removal (or engine removal) was accepted.
stop_bottle() {
  local b="$1" ws="$ENGINE/bin/wineserver"
  [ -d "$BASE/$b" ] || return 0
  if [ "$b" = steam-bottle ]; then
    # This resolves private, legacy and explicit runtimes exactly as play does.
    # A failed stop/wait aborts uninstall before any bottle files are removed.
    python3 "$ROOT/scripts/steam-session.py" stop "$STEAM_WINE_ROOT" "$BASE/$b"
    return
  fi
  WINEPREFIX="$BASE/$b" DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib" "$ws" -k 2>/dev/null || true
  WINEPREFIX="$BASE/$b" DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib" "$ws" -w 2>/dev/null || true
}

# 2. App bundles.
# Only bundles install.sh wrote for this install (their launcher script names
# this engine), so a custom SOJU_BASE never removes another install's apps.
apps=()
for a in "Battle.net" "Steam (Windows)" "Epic Games Launcher" "GOG GALAXY"; do
  l="$HOME/Applications/$a.app/Contents/MacOS/launcher"
  [ -f "$l" ] && grep -q "scripts/play.sh" "$l" && grep -qF "ENGINE=\"$ENGINE\"" "$l" && apps+=("$HOME/Applications/$a.app")
done
if [ "${#apps[@]}" -gt 0 ]; then
  printf 'Apps: %s\n' "${apps[@]}"
  if ask "Remove these apps?"; then rm -rf "${apps[@]}"; REMOVED=1; echo "  removed"; fi
fi

# 3. Bottles (this is where installed games live; can be tens of GB).
for b in bottle steam-bottle epic-bottle gog-bottle; do
  d="$BASE/$b"
  [ -d "$d" ] || continue
  sz=$(du -sh "$d" 2>/dev/null | cut -f1)
  if ask "Remove bottle $b ($sz, includes installed games)?"; then
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

# 4. Engine (+ the GPTK payload inside it).
if [ -d "$ENGINE" ]; then
  sz=$(du -sh "$ENGINE" 2>/dev/null | cut -f1)
  if ask "Remove the engine ($sz, includes the GPTK payload; re-downloadable with soju install)?"; then
    for b in bottle epic-bottle gog-bottle; do stop_bottle "$b"; done
    rm -rf "$ENGINE"; REMOVED=1; echo "  removed"
  fi
fi

# 5. Caches and support files: downloaded installers and sources (build/),
#    compiled helpers (*-support/), logs, and leftovers of an interrupted update.
extras=()
for d in build steam-support epic-support gog-support logs cx26-engine.old cx26-engine.new soju.old; do
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
EOT
echo "Done."
