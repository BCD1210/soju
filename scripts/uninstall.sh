#!/usr/bin/env bash
# soju uninstall: remove what soju created, asking per category.
#   soju uninstall          interactive
#   soju uninstall --yes    remove everything without asking
set -euo pipefail

BASE="${SOJU_BASE:-$HOME/.battlenet-macos}"
ENGINE="${ENGINE:-$BASE/cx26-engine}"
TTY=/dev/tty
YES=0; [ "${1:-}" = "--yes" ] && YES=1

ask(){
  [ "$YES" = 1 ] && return 0
  read -r -p "$1 [y/N]: " a < "$TTY" || a=n
  case "$a" in y|Y|yes) return 0;; *) return 1;; esac
}

echo "soju uninstall: this removes things soju created. Nothing is touched without a yes."

# 1. Stop everything first (best effort).
for b in bottle epic-bottle gog-bottle; do
  [ -d "$BASE/$b" ] && WINEPREFIX="$BASE/$b" DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib" \
    "$ENGINE/bin/wineserver" -k 2>/dev/null || true
done
WS="/Applications/Wine Stable.app/Contents/Resources/wine/bin/wineserver"
[ -d "$BASE/steam-bottle" ] && [ -x "$WS" ] && WINEPREFIX="$BASE/steam-bottle" "$WS" -k 2>/dev/null || true
sleep 1

# 2. App bundles.
apps=()
for a in "Battle.net" "Steam (Windows)" "Epic Games Launcher" "GOG GALAXY"; do
  [ -d "$HOME/Applications/$a.app" ] && apps+=("$HOME/Applications/$a.app")
done
if [ "${#apps[@]}" -gt 0 ]; then
  printf 'Apps: %s\n' "${apps[@]}"
  if ask "Remove these apps?"; then rm -rf "${apps[@]}"; echo "  removed"; fi
fi

# 3. Bottles (this is where installed games live; can be tens of GB).
for b in bottle steam-bottle epic-bottle gog-bottle; do
  d="$BASE/$b"
  [ -d "$d" ] || continue
  sz=$(du -sh "$d" 2>/dev/null | cut -f1)
  if ask "Remove bottle $b ($sz, includes installed games)?"; then rm -rf "$d"; echo "  removed"; fi
done

# 4. Engine (+ the GPTK payload inside it).
if [ -d "$ENGINE" ]; then
  sz=$(du -sh "$ENGINE" 2>/dev/null | cut -f1)
  if ask "Remove the engine ($sz, includes the GPTK payload; re-downloadable with soju install)?"; then
    rm -rf "$ENGINE"; echo "  removed"
  fi
fi

# 5. Caches and support files: downloaded installers and sources (build/),
#    compiled helpers (*-support/), logs, and leftovers of an interrupted update.
extras=()
for d in build steam-support epic-support gog-support logs cx26-engine.old cx26-engine.new soju.old; do
  [ -e "$BASE/$d" ] && extras+=("$BASE/$d")
done
if [ "${#extras[@]}" -gt 0 ]; then
  sz=$(du -shc "${extras[@]}" 2>/dev/null | tail -1 | cut -f1)
  printf 'Caches and support files (%s):\n' "$sz"; printf '  %s\n' "${extras[@]}"
  if ask "Remove these?"; then rm -rf "${extras[@]}"; echo "  removed"; fi
fi
rm -f "$BASE"/reaper-*.log "$BASE"/*.tar.xz "$BASE"/*.part 2>/dev/null || true

# 6. The scripts copy made by the one-line installer.
if [ -d "$BASE/soju" ]; then
  if ask "Remove the scripts copy at $BASE/soju?"; then rm -rf "$BASE/soju"; echo "  removed"; fi
fi
rmdir "$BASE" 2>/dev/null && echo "Removed empty $BASE" || true

cat <<'EOT'

Not removed by this script (they are Homebrew's):
  brew uninstall soju                    the soju CLI itself, if installed via brew
  brew uninstall --cask wine-stable      the Steam engine
  brew uninstall mingw-w64               the wrapper build dependency
EOT
echo "Done."
