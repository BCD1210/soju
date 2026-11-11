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
source "$ROOT/scripts/d2r-macos.sh"
WINESTABLE="$STEAM_WINE_ROOT/bin/wine"

nok=0; nwarn=0; nfail=0
pass(){ printf '  ok    %s\n' "$*"; nok=$((nok+1)); }
warn(){ printf '  warn  %s\n' "$*"; nwarn=$((nwarn+1)); }
fail(){ printf '  FAIL  %s\n' "$*"; nfail=$((nfail+1)); }
info(){ printf '  -     %s\n' "$*"; }

SCOPE="${1:-installed}"
case "$SCOPE" in installed|all|steam|battlenet|d2r|epic|gog|ea|rockstar|ubisoft|xbox) ;; *) echo "Usage: soju doctor [steam|battlenet|d2r|epic|gog|ea|rockstar|ubisoft|xbox|all]"; exit 64 ;; esac
NEEDS_CX=0
case "$SCOPE" in
  all|battlenet|d2r|epic|gog|ea|rockstar|ubisoft) NEEDS_CX=1 ;;
  installed)
    if [ -x "$ENGINE/bin/wine" ] || [ -d "$BASE/bottle" ] || [ -d "$BASE/epic-bottle" ] || [ -d "$BASE/gog-bottle" ] \
       || [ -d "$BASE/ea-bottle" ] || [ -d "$BASE/rockstar-bottle" ] || [ -d "$BASE/ubisoft-bottle" ]; then NEEDS_CX=1; fi ;;
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
# Battle.net can run on an OS where D2R cannot. Make that distinction visible.
CHECK_D2R=0
case "$SCOPE" in
  all|battlenet|d2r) CHECK_D2R=1 ;;
  installed) [ ! -d "$BASE/bottle" ] || CHECK_D2R=1 ;;
esac
if [ "$CHECK_D2R" = 1 ]; then
  if macos_check=$(soju_d2r_macos_check); then
    pass "$macos_check"
  elif [ "$SCOPE" = d2r ]; then
    fail "$macos_check"
  else
    warn "$macos_check"
  fi
fi
echo "Engine ($ENGINE)"
if [ -x "$ENGINE/bin/wine" ]; then
  v=$(DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib" "$ENGINE/bin/wine" --version 2>/dev/null || true)
  if [ -n "$v" ]; then pass "wine runs: $v"; else fail "engine present but wine does not start (DYLD problem? reinstall: soju update)"; fi
  tag=$(cat "$ENGINE/.soju-engine-release" 2>/dev/null || echo "")
  [ -n "$tag" ] && info "installed engine release: $tag" || info "engine release tag not recorded (installed before v1.2 of the installer)"
  # soju update installs only the engine these scripts pin.
  pinned=$(sed -n 's/^ENGINE_TAG="\(.*\)"$/\1/p' "$ROOT/scripts/update.sh" | head -1)
  if [ -z "$pinned" ]; then
    info "engine pin not found in $ROOT/scripts/update.sh"
  elif [ "$pinned" = "$tag" ]; then pass "engine is the release these scripts use ($pinned)"
  else warn "these scripts use engine $pinned, installed is ${tag:-unknown} (run: soju update)"; fi
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
  [ -x "$WINESTABLE" ] && pass "Steam Wine runtime present" \
    || fail "Steam runtime is missing: run soju steam-install"
else
  info "Steam not installed (soju steam-install)"
fi
[ -f "$BASE/epic-bottle/drive_c/Program Files/Epic Games/Launcher/Portal/Binaries/Win64/EpicGamesLauncher.exe" ] \
  && pass "Epic Games Launcher bottle" || info "Epic not installed (soju epic-install)"
[ -f "$BASE/gog-bottle/drive_c/Program Files/GOG Galaxy/GalaxyClient.exe" ] \
  && pass "GOG GALAXY bottle" || info "GOG not installed (soju gog-install)"
LAUNCHERS=0
for l in "ea|EA app|Program Files/Electronic Arts/EA Desktop" \
         "rockstar|Rockstar Games Launcher|Program Files/Rockstar Games/Launcher/Launcher.exe" \
         "ubisoft|Ubisoft Connect|Program Files (x86)/Ubisoft/Ubisoft Game Launcher/UbisoftConnect.exe"; do
  IFS='|' read -r id name client <<<"$l"
  if [ -e "$BASE/$id-bottle/drive_c/$client" ]; then pass "$name bottle"; LAUNCHERS=1
  else info "$name not installed (soju $id-install)"; fi
done
if [ "$LAUNCHERS" = 1 ]; then
  if SOJU_BASE="$BASE" python3 "$(dirname "${BASH_SOURCE[0]}")/fetch-launcher-support.py" --check; then
    pass "Launcher components (Mac driver + DXMT for EA, Rockstar, Ubisoft)"
  else
    info "Launcher components not downloaded yet (they are fetched when one of these launchers starts)"
  fi
  [ -f "$BASE/cx26-engine-launchers/.soju-launcher-engine" ] && pass "Launcher engine prepared" \
    || info "Launcher engine not prepared yet (made from the engine when one of these launchers starts)"
fi

CHECK_XBOX=0
case "$SCOPE" in
  all|xbox) CHECK_XBOX=1 ;;
  installed) if [ -d "$BASE/xodus" ] || [ -d "$BASE/xbox-bottle" ] || [ -n "${SOJU_XODUS_BIN:-}" ]; then CHECK_XBOX=1; fi ;;
esac
XODUS_CLI="${SOJU_XODUS_BIN:-$BASE/xodus/bin/xodus-cli}"
if [ "$CHECK_XBOX" = 1 ] && [ "$SCOPE" = all ] && [ ! -e "$XODUS_CLI" ] && [ ! -d "$BASE/xbox-bottle" ]; then
  info "Xbox PC not installed (soju xbox-install)"
elif [ "$CHECK_XBOX" = 1 ]; then
  echo
  echo "Xbox PC (Xodus backend)"
  if [ -x "$XODUS_CLI" ] && XODUS_LOG=error "$XODUS_CLI" --version >/dev/null 2>&1; then
    pass "xodus-cli runs ($XODUS_CLI)"
    if XODUS_LOG=error "$XODUS_CLI" library --help >/dev/null 2>&1; then pass "library and download commands present"
    else fail "xodus-cli lacks the library command (rebuild: soju xbox-install)"; fi
    if [ -z "${SOJU_XODUS_BIN:-}" ]; then
      python3 "$ROOT/scripts/fetch-xodus-backend.py" --check \
        && pass "backend matches the pinned recipe" || warn "backend recipe changed: run soju update or soju xbox-install"
    else
      info "custom backend from SOJU_XODUS_BIN"
    fi
  else
    fail "Xodus backend not built (run: soju xbox-install)"
  fi
  [ -f "$BASE/xbox-bottle/drive_c/ProgramData/Soju/xbox.json" ] && pass "Xbox PC folder prepared" \
    || warn "Xbox PC folder missing (run: soju xbox-install)"
  # The backend says only whether an account is stored; the account name is not printed.
  if [ -x "$XODUS_CLI" ] && XODUS_LOG=error "$XODUS_CLI" account --json 2>/dev/null | grep -q '"stored":true'; then
    pass "signed in to a Microsoft account"
  else
    info "not signed in to Microsoft (Accounts > Xbox PC, or soju xbox-login)"
  fi
  if [ -f "$BASE/account-libraries/xbox.json" ]; then
    n=$(python3 -c 'import json,sys;print(len(json.load(open(sys.argv[1]))["games"]))' "$BASE/account-libraries/xbox.json" 2>/dev/null || echo "?")
    info "imported Xbox library: $n games"
  fi
  # Download folders (the Mac and external drives); paths are printed with ~ for the home folder.
  while IFS="$(printf '\t')" read -r level text; do
    case "$level" in warn) warn "$text" ;; *) info "$text" ;; esac
  done < <(SOJU_BASE="$BASE" python3 "$ROOT/scripts/xbox_locations.py" json 2>/dev/null | python3 -c '
import json, sys
try:
    v = json.load(sys.stdin)
except ValueError:
    sys.exit(0)
print("info\tXbox downloads go to " + v["current"]["path"] + ("" if v["current"]["available"] else " (not connected)"))
partial = sum(r["partial"] for r in v["roots"])
if partial: print("warn\t" + str(partial) + " unfinished Xbox download(s): download again to resume")
for r in v["roots"]:
    if not r["connected"]: print("warn\tXbox game drive not connected: " + r["volume"])
for w in v["warnings"]: print("info\t" + w)
')
  python3 "$ROOT/scripts/fetch-xbox-runtime.py" --check && pass "Xbox game runtime DLLs verified" \
    || info "Xbox game runtime not downloaded yet (first Play or soju xbox-install fetches it)"
  python3 "$ROOT/scripts/xbox-prefix.py" --check && pass "Xbox prefix ready to play" \
    || info "Xbox prefix not prepared yet (needs the CX engine and GPTK; first Play prepares it)"
fi

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
elif [ "$nwarn" -gt 0 ]; then
  echo "No failed environment checks, but review the warnings above before playing."
else
  echo "Environment checks passed; game compatibility is not verified. If Soju works for you, a star helps other Mac gamers find it: https://github.com/$REPO"
fi
[ "$nfail" -eq 0 ]
