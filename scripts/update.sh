#!/usr/bin/env bash
# soju update: update the scripts and the prebuilt engine in place.
# GPTK files (lib/external + symlinks) and all bottles are preserved.
set -euo pipefail

REPO="BCD1210/soju"
BASE="${SOJU_BASE:-$HOME/.battlenet-macos}"
ENGINE="${ENGINE:-$BASE/cx26-engine}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TTY=/dev/tty

say(){ printf '\n\033[1m%s\033[0m\n' "$*"; }

# ---------- 1. Scripts ----------
# Once the scripts are refreshed, the rest of this run must use the new ones
# (the engine carry-over in particular changes between versions), so step 1
# re-executes the fresh update.sh with this marker and skips itself.
if [ "${1:-}" = "--scripts-updated" ]; then
  shift
  say "[1/2] Scripts updated"
else
say "[1/2] Updating the Soju scripts"
if [ -e "$ROOT/.git" ]; then          # a directory, or a file for worktrees/submodules
  echo "  git checkout at $ROOT"
  git -C "$ROOT" pull --ff-only || echo "  git pull failed (local changes?); skipping the scripts update"
elif [[ "$ROOT" == */Cellar/* || "$ROOT" == */opt/soju/* ]]; then
  echo "  installed with Homebrew; update with:  brew update && brew upgrade $REPO/soju"
elif [ "$ROOT" != "$BASE/soju" ]; then
  # Only the copy install.sh made is ours to overwrite; a zip download or any
  # other checkout may carry local edits.
  echo "  $ROOT is not the installer's copy ($BASE/soju); update it yourself (git pull, or re-download)"
else
  echo "  refreshing the tarball copy at $ROOT"
  tmp="$ROOT.tmp.$$"
  mkdir -p "$tmp"
  if curl -fsSL "https://github.com/$REPO/archive/refs/heads/main.tar.gz" | tar -xz -C "$tmp" --strip-components=1; then
    # Swap directories instead of copying over the running script.
    chmod +x "$tmp"/scripts/*.sh "$tmp"/scripts/soju 2>/dev/null || true
    rm -rf "$ROOT.old"; mv "$ROOT" "$ROOT.old"; mv "$tmp" "$ROOT"; rm -rf "$ROOT.old"
    tmp=""
    echo "  scripts updated"
    exec bash "$ROOT/scripts/update.sh" --scripts-updated "$@"
  else
    echo "  download failed; skipping the scripts update"
  fi
  [ -n "$tmp" ] && rm -rf "$tmp"
fi
fi

# ---------- 2. Engine ----------
say "[2/2] Checking the engine"
if [ ! -x "$ENGINE/bin/wine" ]; then
  echo "  no engine at $ENGINE (run: soju install)"; exit 0
fi

URL=$(curl -fsSL "https://api.github.com/repos/$REPO/releases?per_page=30" \
      | grep -o '"browser_download_url": *"[^"]*soju-engine[^"]*"' | head -1 | grep -o 'https[^"]*') || true
[ -n "${URL:-}" ] || { echo "  could not find the engine release asset (offline?)"; exit 1; }
TAG=$(echo "$URL" | sed -n 's#.*/download/\([^/]*\)/.*#\1#p')
CUR=$(cat "$ENGINE/.soju-engine-release" 2>/dev/null || echo "")

if [ -n "$CUR" ] && [ "$CUR" = "$TAG" ]; then
  echo "  engine is up to date ($TAG)"; exit 0
fi
if [ -z "$CUR" ]; then
  echo "  installed engine version is not recorded; latest is $TAG"
  if [ -e "$TTY" ] && [ -z "${SOJU_UPDATE_ENGINE:-}" ]; then
    read -r -p "  Re-download the engine to be sure? (~350MB) [Y/n]: " a < "$TTY" || a=n
    case "$a" in n|N) echo "  keeping the current engine"; exit 0;; esac
  fi
else
  echo "  $CUR -> $TAG"
fi

# Processes already running keep the old engine mapped while new ones (a game
# started from the launcher, Agent) would load the new one against the old
# wineserver. Refuse rather than mix them.
if pgrep -x wineserver >/dev/null 2>&1; then
  echo "  a launcher or game is still running; close it first (soju kill / epic-kill / gog-kill / steam-kill) and run soju update again"
  exit 1
fi

echo "  downloading $TAG"
curl -fL "$URL" -o "$BASE/engine.tar.xz.part"
mv -f "$BASE/engine.tar.xz.part" "$BASE/engine.tar.xz"
NEW="$ENGINE.new"
rm -rf "$NEW"; mkdir -p "$NEW"
tar -xJf "$BASE/engine.tar.xz" -C "$NEW"
rm -f "$BASE/engine.tar.xz"
printf '%s\n' "$TAG" > "$NEW/.soju-engine-release"

# Carry the GPTK payload over: lib/external, Apple's PE shims in
# x86_64-windows (d3d11/d3d12/dxgi/...: without them Wine's own d3d11 runs on
# wined3d and the Epic launcher crashes at start), and one unixlib symlink per
# shim (symlinks, never copies: copying breaks @loader_path).
if [ -f "$ENGINE/lib/external/libd3dshared.dylib" ]; then
  mkdir -p "$NEW/lib/external"
  cp -Rf "$ENGINE/lib/external/." "$NEW/lib/external/"
  for f in "$ENGINE"/lib/wine/x86_64-unix/*.so; do
    [ -L "$f" ] || continue
    b=$(basename "$f" .so)
    if [ -f "$ENGINE/lib/wine/x86_64-windows/$b.dll" ]; then
      [ -f "$NEW/lib/wine/x86_64-windows/$b.dll" ] && cp -p "$NEW/lib/wine/x86_64-windows/$b.dll" "$NEW/lib/wine/x86_64-windows/$b.dll.wine"
      cp -f "$ENGINE/lib/wine/x86_64-windows/$b.dll" "$NEW/lib/wine/x86_64-windows/$b.dll"
    fi
    ln -sf ../../external/libd3dshared.dylib "$NEW/lib/wine/x86_64-unix/$b.so"
  done
fi

if ! DYLD_FALLBACK_LIBRARY_PATH="$NEW/lib:/usr/lib" "$NEW/bin/wine" --version >/dev/null 2>&1; then
  echo "  new engine does not start; keeping the current one (new copy left at $NEW)"; exit 1
fi

OLD="$ENGINE.old"
rm -rf "$OLD"
mv "$ENGINE" "$OLD"
mv "$NEW" "$ENGINE"
rm -rf "$OLD"
echo "  engine updated to $TAG: $(DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib" "$ENGINE/bin/wine" --version)"
