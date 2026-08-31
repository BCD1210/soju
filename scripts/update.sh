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
say "[1/2] Updating the Soju scripts"
if [ -d "$ROOT/.git" ]; then
  echo "  git checkout at $ROOT"
  git -C "$ROOT" pull --ff-only || echo "  git pull failed (local changes?); skipping the scripts update"
elif [[ "$ROOT" == */Cellar/* ]]; then
  echo "  installed with Homebrew; update with:  brew update && brew upgrade $REPO/soju"
else
  echo "  refreshing the tarball copy at $ROOT"
  tmp="$ROOT.tmp.$$"
  mkdir -p "$tmp"
  if curl -fsSL "https://github.com/$REPO/archive/refs/heads/main.tar.gz" | tar -xz -C "$tmp" --strip-components=1; then
    rsync -a --delete --exclude '.git' "$tmp/" "$ROOT/" 2>/dev/null || { cp -Rf "$tmp/". "$ROOT/"; }
    chmod +x "$ROOT"/scripts/*.sh "$ROOT"/scripts/soju 2>/dev/null || true
    echo "  scripts updated"
  else
    echo "  download failed; skipping the scripts update"
  fi
  rm -rf "$tmp"
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

echo "  downloading $TAG"
curl -fL "$URL" -o "$BASE/engine.tar.xz"
NEW="$ENGINE.new"
rm -rf "$NEW"; mkdir -p "$NEW"
tar -xJf "$BASE/engine.tar.xz" -C "$NEW"
rm -f "$BASE/engine.tar.xz"
printf '%s\n' "$TAG" > "$NEW/.soju-engine-release"

# Carry the GPTK payload over and restore the symlink layout (real files in
# lib/external, symlinks in x86_64-unix; copying real files there breaks
# @loader_path, see the README).
if [ -f "$ENGINE/lib/external/libd3dshared.dylib" ]; then
  mkdir -p "$NEW/lib/external"
  cp -Rf "$ENGINE/lib/external/." "$NEW/lib/external/"
  ( cd "$NEW/lib/wine/x86_64-unix" \
    && for f in d3d10.so d3d11.so d3d12.so dxgi.so; do ln -sf ../../external/libd3dshared.dylib "$f"; done )
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
