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

# Same pin and helpers as install.sh (tests/test_installation.py keeps them equal).
ENGINE_TAG="engine-v1.8"
ENGINE_SHA256="6e45b119cb01bf9e6ca8bca7cd1f1dd74a88611c6bb5146e33ec98cfc17a3dbf"
ENGINE_URL="https://github.com/$REPO/releases/download/$ENGINE_TAG/wine-engine-x86_64.tar.xz"

fetch_verified(){   # url, dest, sha256
  local got
  rm -f "$2.part"
  curl -fL "$1" -o "$2.part" || { rm -f "$2.part"; return 1; }
  got=$(shasum -a 256 "$2.part" | awk '{print $1}')
  if [ "$got" != "$3" ]; then
    rm -f "$2.part"
    printf 'Checksum mismatch for %s\n  expected %s\n  got      %s\n' "$1" "$3" "$got"
    return 1
  fi
  mv -f "$2.part" "$2"
}

latest_release(){
  local json i j tag name n digest
  json=$(mktemp) || return 1
  if ! curl -fsSL "https://api.github.com/repos/$REPO/releases?per_page=30" -o "$json"; then
    rm -f "$json"; return 1
  fi
  for i in $(seq 0 29); do
    tag=$(plutil -extract "$i.tag_name" raw -o - "$json" 2>/dev/null) || break
    [[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || continue
    [ "$(plutil -extract "$i.prerelease" raw -o - "$json" 2>/dev/null)" = false ] || continue
    name="Soju-${tag#v}-macos-arm64.zip"
    for j in $(seq 0 19); do
      n=$(plutil -extract "$i.assets.$j.name" raw -o - "$json" 2>/dev/null) || break
      [ "$n" = "$name" ] || continue
      digest=$(plutil -extract "$i.assets.$j.digest" raw -o - "$json" 2>/dev/null) || break
      [[ "$digest" =~ ^sha256:[0-9a-f]{64}$ ]] || break
      echo "$tag https://github.com/$REPO/releases/download/$tag/$name ${digest#sha256:}"
      rm -f "$json"; return 0
    done
  done
  rm -f "$json"; return 1
}

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
  # The installer's copy follows releases, never main: take the newest release
  # zip, check its SHA-256 and swap in the scripts it carries.
  cur="v$(cat "$ROOT/VERSION" 2>/dev/null || echo unknown)"
  if ! rel=$(latest_release); then
    echo "  could not find a release with a recorded checksum (offline?); skipping the scripts update"
  elif [ "${rel%% *}" = "$cur" ]; then
    echo "  scripts are up to date ($cur)"
  else
    read -r tag url sum <<<"$rel"
    echo "  $cur -> $tag"
    tmp="$ROOT.tmp.$$"
    rm -rf "$tmp"; mkdir -p "$tmp"
    if fetch_verified "$url" "$tmp/soju.zip" "$sum" \
       && ditto -x -k "$tmp/soju.zip" "$tmp/app" \
       && [ -f "$tmp/app/Soju.app/Contents/Resources/soju/scripts/update.sh" ]; then
      new="$tmp/app/Soju.app/Contents/Resources/soju"
      chmod +x "$new"/install.sh "$new"/scripts/*.sh "$new"/scripts/soju 2>/dev/null || true
      # Swap directories instead of copying over the running script.
      rm -rf "$ROOT.old"; mv "$ROOT" "$ROOT.old"; mv "$new" "$ROOT"; rm -rf "$ROOT.old" "$tmp"
      echo "  scripts updated"
      exec bash "$ROOT/scripts/update.sh" --scripts-updated "$@"
    fi
    echo "  download or check failed; skipping the scripts update"
    rm -rf "$tmp"
  fi
fi
fi

# Engines set up from an older toolkit, or by an older Soju, can have only
# d3d11/d3d12/dxgi of Apple's shims. Games that look for the AMD and NVIDIA
# driver shims (Overwatch 2: "General Error 0xE0010110" without atidxx64.dll)
# then stop at start. Fill in the rest from the pinned toolkit download.
complete_gptk(){
  local f missing=""
  [ -f "$ENGINE/lib/external/libd3dshared.dylib" ] || return 0
  [ "${SOJU_GPTK_DOWNLOAD:-1}" = 1 ] || return 0
  for f in atidxx64 nvapi64; do
    grep -q "D3DMetalDLLsBase" "$ENGINE/lib/wine/x86_64-windows/$f.dll" 2>/dev/null || missing="$missing $f"
  done
  [ -n "$missing" ] || return 0
  if pgrep -x wineserver >/dev/null 2>&1; then
    echo "  Apple GPTK is missing$missing.dll; close launchers and games and run soju update again to add them"
    return 0
  fi
  echo "  adding Apple's GPTK driver shims ($missing )"
  ENGINE="$ENGINE" SOJU_BASE="$BASE" python3 "$ROOT/scripts/fetch-gptk.py" || echo "  could not add them; games that need them may not start (soju gptk retries)"
}

# ---------- 2. Engine ----------
say "[2/2] Checking the engine"
if [ ! -x "$ENGINE/bin/wine" ]; then
  if [ -f "$BASE/steam-bottle/drive_c/Program Files (x86)/Steam/steam.exe" ]; then
    echo "  Steam-only installation: CX engine / GPTK not needed"
    exec bash "$ROOT/scripts/setup-steam-games.sh"
  fi
  echo "  no launchers installed (run: soju install)"; exit 0
fi

# The engine is pinned (ENGINE_TAG/ENGINE_SHA256 above): moving users to a new
# engine release takes a script release that names it.
TAG="$ENGINE_TAG"
CUR=$(cat "$ENGINE/.soju-engine-release" 2>/dev/null || echo "")

if [ -n "$CUR" ] && [ "$CUR" = "$TAG" ]; then
  echo "  engine is up to date ($TAG)"; complete_gptk; exit 0
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
fetch_verified "$ENGINE_URL" "$BASE/engine.tar.xz" "$ENGINE_SHA256" || { echo "  keeping the current engine"; exit 1; }
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
  shims=0
  for f in "$ENGINE"/lib/wine/x86_64-unix/*.so; do
    [ -L "$f" ] || continue
    b=$(basename "$f" .so)
    # Only Apple's shims travel (they carry Apple's build path string). A Wine
    # DLL beside a symlink is an engine set up by an older get-gptk.sh; copying
    # it would just move wined3d over.
    grep -q "D3DMetalDLLsBase" "$ENGINE/lib/wine/x86_64-windows/$b.dll" 2>/dev/null || continue
    [ -f "$NEW/lib/wine/x86_64-windows/$b.dll" ] && cp -p "$NEW/lib/wine/x86_64-windows/$b.dll" "$NEW/lib/wine/x86_64-windows/$b.dll.wine"
    cp -f "$ENGINE/lib/wine/x86_64-windows/$b.dll" "$NEW/lib/wine/x86_64-windows/$b.dll"
    ln -sf ../../external/libd3dshared.dylib "$NEW/lib/wine/x86_64-unix/$b.so"
    shims=$((shims + 1))
  done
  if [ "$shims" -eq 0 ]; then
    echo "  NOTE: the old engine had the GPTK Metal side but not Apple's D3D shims, so D3DMetal was not"
    echo "        active. Mount the GPTK dmg (or have CrossOver installed) and run:  soju gptk"
  fi
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
complete_gptk
