#!/usr/bin/env bash
# soju doctor: check the whole stack and print what is wrong. Read-only.
# Paste this output when filing an issue.
set -uo pipefail

BASE="${SOJU_BASE:-$HOME/.battlenet-macos}"
ENGINE="${ENGINE:-$BASE/cx26-engine}"
REPO="BCD1210/soju"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOJU="$ROOT/scripts/soju"
source "$ROOT/scripts/steam-runtime.sh"
WINESTABLE="$STEAM_WINE_ROOT/bin/wine"

nok=0; nwarn=0; nfail=0
pass(){ printf '  ok    %s\n' "$*"; nok=$((nok+1)); }
warn(){ printf '  warn  %s\n' "$*"; nwarn=$((nwarn+1)); }
fail(){ printf '  FAIL  %s\n' "$*"; nfail=$((nfail+1)); }
info(){ printf '  -     %s\n' "$*"; }

SCOPE="${1:-installed}"
case "$SCOPE" in installed|all|steam|battlenet|epic|gog) ;; *) echo "Usage: soju doctor [steam|battlenet|epic|gog|all]"; exit 64 ;; esac
NEEDS_CX=0
case "$SCOPE" in
  all|battlenet|epic|gog) NEEDS_CX=1 ;;
  installed)
    if [ -x "$ENGINE/bin/wine" ] || [ -d "$BASE/bottle" ] || [ -d "$BASE/epic-bottle" ] || [ -d "$BASE/gog-bottle" ]; then NEEDS_CX=1; fi ;;
esac
echo "soju doctor ($SCOPE)"

echo
echo "System"
info "macOS $(sw_vers -productVersion) ($(uname -m)), $(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo unknown CPU)"
[ "$(uname -m)" = "arm64" ] && pass "Apple Silicon" || fail "not Apple Silicon: this stack is arm64-only"
/usr/bin/pgrep -q oahd && pass "Rosetta 2 running" || fail "Rosetta 2 missing: softwareupdate --install-rosetta"
xcode-select -p >/dev/null 2>&1 && pass "Xcode Command Line Tools" || warn "Xcode CLT missing: xcode-select --install"
free_gb=$(df -g "$HOME" | awk 'NR==2 {print $4}')
if [ "${free_gb:-0}" -lt 20 ]; then warn "only ${free_gb}GB free on the home volume (games are big)"; else pass "${free_gb}GB free disk"; fi

echo
if [ "$NEEDS_CX" = 1 ]; then
echo "Engine ($ENGINE)"
if [ -x "$ENGINE/bin/wine" ]; then
  v=$(DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib" "$ENGINE/bin/wine" --version 2>/dev/null || true)
  if [ -n "$v" ]; then pass "wine runs: $v"; else fail "engine present but wine does not start (DYLD problem? reinstall: soju update)"; fi
  tag=$(cat "$ENGINE/.soju-engine-release" 2>/dev/null || echo "")
  [ -n "$tag" ] && info "installed engine release: $tag" || info "engine release tag not recorded (installed before v1.2 of the installer)"
  latest=$(curl -fsSL --max-time 5 "https://api.github.com/repos/$REPO/releases?per_page=30" 2>/dev/null \
           | grep -o '"tag_name": *"engine-[^"]*"' | head -1 | cut -d'"' -f4 || true)
  if [ -n "$latest" ]; then
    if [ "$latest" = "$tag" ]; then pass "engine is the latest release ($latest)"
    else warn "latest engine release is $latest (run: soju update)"; fi
  else
    info "could not reach GitHub to compare engine versions (offline?)"
  fi
else
  fail "engine not installed (run: soju install)"
fi

echo
echo "Apple GPTK (D3DMetal)"
if [ -f "$ENGINE/lib/external/libd3dshared.dylib" ]; then
  pass "libd3dshared.dylib present"
  [ -d "$ENGINE/lib/external/D3DMetal.framework" ] && pass "D3DMetal.framework present" \
    || warn "D3DMetal.framework missing from lib/external ($SOJU gptk installs it)"
  for f in d3d11 d3d12 dxgi; do
    so="$ENGINE/lib/wine/x86_64-unix/$f.so"
    if [ -L "$so" ]; then pass "$f.so is a symlink"
    elif [ -f "$so" ]; then fail "$f.so is a regular file: must be a symlink into lib/external (copying breaks @loader_path); re-run: $SOJU gptk"
    else warn "$f.so missing"; fi
    # Apple's PE shim carries its build path; Wine's own d3d11.dll does not.
    dll="$ENGINE/lib/wine/x86_64-windows/$f.dll"
    if [ -f "$dll" ] && grep -q "D3DMetalDLLsBase" "$dll" 2>/dev/null; then pass "$f.dll is the D3DMetal shim"
    else fail "$f.dll is Wine's own, not Apple's D3DMetal shim: D3D runs on wined3d and the Epic launcher crashes (re-run: $SOJU gptk)"; fi
  done
else
  fail "GPTK not installed: games will not start (run: $SOJU gptk)"
fi

echo
else
  info "CX engine / Apple GPTK not required for the selected installation"
fi
echo "Bottles ($BASE)"
[ -f "$BASE/bottle/drive_c/Program Files (x86)/Battle.net/Battle.net.exe" ] \
  && pass "Battle.net bottle" || info "Battle.net not installed (soju install)"
if [ -f "$BASE/steam-bottle/drive_c/Program Files (x86)/Steam/steam.exe" ]; then
  pass "Steam bottle"
  if [ -f "$BASE/steam-runtime/.soju-runtime" ] && [ -f "$BASE/steam-bottle/.soju-steam-games" ]; then
    pass "Soju private Steam runtime / D3D11 components installed"
  elif [ -f "$BASE/steam-support/winemac-patched.so" ]; then
    info "Legacy Steam renderer present; close Steam and run soju steam-games to migrate"
  else
    warn "Steam game rendering not configured: soju steam-games"
  fi
  [ -x "$WINESTABLE" ] && pass "Homebrew wine-stable present" \
    || fail "Steam bottle exists but wine-stable is gone: brew install --cask wine-stable"
else
  info "Steam not installed (soju steam-install)"
fi
[ -f "$BASE/epic-bottle/drive_c/Program Files/Epic Games/Launcher/Portal/Binaries/Win64/EpicGamesLauncher.exe" ] \
  && pass "Epic Games Launcher bottle" || info "Epic not installed (soju epic-install)"
[ -f "$BASE/gog-bottle/drive_c/Program Files/GOG Galaxy/GalaxyClient.exe" ] \
  && pass "GOG GALAXY bottle" || info "GOG not installed (soju gog-install)"

echo
echo "Processes"
# macOS pgrep has no -c: count lines instead.
nsrv=$(pgrep -x wineserver 2>/dev/null | wc -l | tr -d ' ')
if [ "$nsrv" -gt 0 ]; then
  info "$nsrv wineserver(s) running (a launcher or game is open)"
else
  if pgrep -f 'services\.exe|winedevice\.exe|plugplay\.exe' >/dev/null 2>&1; then
    warn "orphaned Wine service processes with no wineserver: run: $SOJU sweep"
  else
    pass "no leftover Wine processes"
  fi
fi
src=$(defaults read ~/Library/Preferences/com.apple.HIToolbox.plist AppleSelectedInputSources 2>/dev/null | grep -o '"KeyboardLayout Name" = [^;]*' | head -1 | cut -d= -f2 | tr -d ' ";')
[ -n "$src" ] && info "current input source: $src (use English/ABC while in games: IMEs swallow key presses)"

echo
echo "Summary: $nok ok, $nwarn warn, $nfail fail"
if [ "$nfail" -gt 0 ]; then
  echo "Paste this whole output in a new issue: https://github.com/$REPO/issues/new"
else
  echo "All good. If Soju works for you, a star helps other Mac gamers find it: https://github.com/$REPO"
fi
[ "$nfail" -eq 0 ]
