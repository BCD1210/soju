#!/usr/bin/env bash
# Diablo II: Resurrected: fully free stack launcher (final verified combination)
#
# Stack: self-built wine 11.0 (CrossOver 26.3 GPL sources) + D3DMetal (Apple GPTK)
# Verified 2026-08-27: Battle.net login / Agent / D2R in-game.
#
# The three keys (finding these took days):
#  1) ROSETTA_ADVERTISE_AVX=1: D2R's loader requires AVX. Without it the game
#     waits forever (stuck at ~86MB RAM).
#  2) D3DMetal symlink layout: real files in lib/external, symlinks in
#     x86_64-unix. Copying breaks @loader_path and causes an assertion loop
#     (frankea's README warns about this).
#  3) Never put Apple-protected binaries (nohup/arch, ...) in the launch chain:
#     macOS strips DYLD_* when exec'ing them.
set -euo pipefail

ENGINE="${ENGINE:-${SOJU_BASE:-$HOME/.battlenet-macos}/cx26-engine}"
MODE="${1:-battlenet}"
source "$(dirname "${BASH_SOURCE[0]}")/steam-runtime.sh"

# Bottle (virtual C: drive) path: per-mode default (Battle.net and Steam use separate bottles)
if [[ "$MODE" == steam* ]]; then
  export WINEPREFIX="${WINEPREFIX:-${SOJU_BASE:-$HOME/.battlenet-macos}/steam-bottle}"
elif [[ "$MODE" == epic* ]]; then
  export WINEPREFIX="${WINEPREFIX:-${SOJU_BASE:-$HOME/.battlenet-macos}/epic-bottle}"
elif [[ "$MODE" == gog* ]]; then
  export WINEPREFIX="${WINEPREFIX:-${SOJU_BASE:-$HOME/.battlenet-macos}/gog-bottle}"
else
  export WINEPREFIX="${WINEPREFIX:-${SOJU_BASE:-$HOME/.battlenet-macos}/bottle}"
fi

export WINEDEBUG="${WINEDEBUG:-fixme-all}"
# SOJU_KEYLOG=1: record every key event the Mac driver sees and every focus
# change, to ~/.battlenet-macos/logs/keys-<mode>-<time>.log. For "a key stays
# pressed" / "keyboard dead, mouse fine" reports: the log shows which window a
# KEY_RELEASE went to. Noisy (mouse moves too); leave it off otherwise.
if [[ "${SOJU_KEYLOG:-0}" == 1 ]]; then
  mkdir -p "${SOJU_BASE:-$HOME/.battlenet-macos}/logs"
  KEYLOG="${SOJU_BASE:-$HOME/.battlenet-macos}/logs/keys-$MODE-$(date +%Y%m%d-%H%M%S).log"
  export WINEDEBUG="fixme-all,+key,+event"
  exec 2>>"$KEYLOG"
  echo "soju keylog $(date) mode=$MODE prefix=$WINEPREFIX" >&2
  echo "soju: key log -> $KEYLOG"
fi
export WINEMSYNC=1
export ROSETTA_ADVERTISE_AVX=1
# Battle.net's CEF renderer CHECKs that VirtualProtect() on a written .data page
# of libcef.dll reports PAGE_READWRITE. Plain wine reports PAGE_WRITECOPY for
# image pages forever (it maps them RW and never tracks the first write), so the
# renderer hits int3 on startup and the login webview stays blank. CrossOver's
# ntdll enables the "simulate writecopy" hack (CW Hack 22996) by default; the
# GPL source only enables it through this env var. See docs/DIAGNOSIS.md.
export WINE_SIMULATE_WRITECOPY=1
export CX_ACTIVE_GRAPHICS_BACKEND=d3dmetal
export CX_GRAPHICS_BACKEND=d3dmetal
export CX_APPLEGPTK_LIBD3DSHARED_PATH="$ENGINE/lib/external/libd3dshared.dylib"
export DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib"

# Agent signature-check fix: sync the signed exe into each versioned subfolder
BN="$WINEPREFIX/drive_c/Program Files (x86)/Battle.net"
if [[ -f "$BN/Battle.net.exe" ]]; then
  for v in "$BN"/Battle.net.[0-9]*; do
    [[ -d "$v" ]] && cp -cf "$BN/Battle.net.exe" "$v/Battle.net.exe" 2>/dev/null || true
  done
fi

REAPER="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/soju-reaper.sh"
SWEEP="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/soju-sweep.sh"   # orphaned-service cleanup
start_reaper(){
  # Watchdog: reaps game processes that outlive their windows, and shuts the
  # prefix down once everything is closed: so "quit" really means quit.
  pgrep -f "soju-reaper.sh $WINEPREFIX" >/dev/null 2>&1 && return 0
  [ -x "$REAPER" ] && ( "$REAPER" "$WINEPREFIX" "$ENGINE/bin/wineserver" >/dev/null 2>&1 & )
  return 0   # a missing reaper must not abort the launch (set -e)
}

# A Korean/Japanese/Chinese IME swallows key presses in Wine games, and macOS
# remembers the input source per app, so switch to ABC before launching.
case "$MODE" in *-kill) ;; *) python3 "$(dirname "${BASH_SOURCE[0]}")/soju-input-abc.py" 2>/dev/null || true ;; esac

# Preflight for the CX engine modes: without the GPTK payload, or with a
# payload whose PE shims are missing (an engine refreshed by an older
# update.sh, which carried only lib/external over), D3D runs on wined3d: the
# Epic launcher crashes at start there, D2R's loader needs libd3dshared to get
# through Rosetta, and everything else is slow. Say so instead of launching.
case "$MODE" in
  battlenet|d2r|epic|gog)
    gptk_ok(){ local f
      [ -f "$ENGINE/lib/external/libd3dshared.dylib" ] || return 1
      for f in d3d11 d3d12 dxgi; do
        grep -q "D3DMetalDLLsBase" "$ENGINE/lib/wine/x86_64-windows/$f.dll" 2>/dev/null || return 1
        [ -L "$ENGINE/lib/wine/x86_64-unix/$f.so" ] || return 1
      done; }
    if ! gptk_ok; then
      if [ -f "$ENGINE/lib/external/libd3dshared.dylib" ]; then
        echo "soju: this engine has the GPTK payload but not Apple's D3D shims (d3d11/d3d12/dxgi.dll), so D3DMetal is not active." >&2
      else
        echo "soju: Apple's Game Porting Toolkit is not installed in this engine, so D3DMetal is not active." >&2
      fi
      echo "      Mount the GPTK dmg (or have CrossOver installed) and run:  $(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/soju gptk" >&2
      exit 1
    fi ;;
esac

case "$MODE" in
  battlenet)   # Battle.net launcher (log in, then Play for online)
    # Battle.net.exe is started directly, not through "Battle.net Launcher.exe":
    # CrossOver's private compat DB (compatdb-*.dat) injects
    # --in-process-gpu --use-gl=swiftshader for Battle.net.exe. Without them CEF
    # spawns a separate GPU process that dies on init and the frameless main
    # window stays fully transparent (Dock icon, no window). See docs/DIAGNOSIS.md.
    start_reaper
    # By default Battle.net exits when its window is closed (same as Windows).
    # With Settings > "When I close the app" set to the tray (Client.HideOnClose),
    # the window hides instead; a Dock-icon click then starts a second
    # Battle.net.exe, which hands off to the running one and shows its window.
    BN_EXE="C:\\Program Files (x86)\\Battle.net\\Battle.net.exe"
    BN_ARGS="--disable-gpu-compositing --from-launcher --in-process-gpu --use-gl=swiftshader"
    export WINE_DOCK_REOPEN_CMD="'$ENGINE/bin/wine' '$BN_EXE' $BN_ARGS"
    exec "$ENGINE/bin/wine" "$BN_EXE" $BN_ARGS "${@:2}"
    ;;
  d2r)         # Launch the game directly (offline / previous session)
    start_reaper
    cd "$WINEPREFIX/drive_c/Program Files (x86)/Diablo II Resurrected"
    exec "$ENGINE/bin/wine" "D2R.exe" "${@:2}"
    ;;
  epic)        # Epic Games Launcher, same engine and env as Battle.net (verified 2026-08-29)
    # Runs as-is: Epic's CEF (EpicWebHelper) keeps its GPU process alive here,
    # so none of the Battle.net command-line switches are needed.
    # Closing the window hides it, exactly like on Windows: Epic's tray icon
    # lands in the macOS menu bar (winemac systray -> NSStatusItem); a
    # double-click or the right-click menu there reopens it. Do NOT force the
    # hidden window back via ShowWindow()/SC_RESTORE from outside: Slate keeps
    # its "minimized" state and the window comes back unresponsive. A Dock-icon
    # click therefore replays the tray double-click into the launcher instead
    # (tools/soju-epic-restore.c, built by create-epic-bottle.sh).
    export WINE_DOCK_REOPEN_CMD="'$ENGINE/bin/wine' '${SOJU_BASE:-$HOME/.battlenet-macos}/epic-support/soju-epic-restore.exe'"
    # The EOS overlay (EOSOVH-*-Shipping.dll, loaded by the EOS SDK into every
    # game, plus a CEF renderer tree beside it) is off by default, like Steam's
    # gameoverlayrenderer. It draws inside the game's own process, so when one
    # of its windows takes Wine's keyboard focus (a toast, an achievement) the
    # game gets no deactivation: a movement key held at that moment stays down,
    # and every key after it goes to the overlay while the mouse keeps working
    # (mouse input follows the cursor, keyboard input follows the foreground
    # window). Under Wine the overlay offers nothing but those toasts. Wine
    # honours the override for a load by full path too (verified with
    # regsvr32: "Failed to load DLL"). SOJU_EPIC_OVERLAY=1 turns it back on.
    # A launcher already running keeps its old environment: soju epic-kill first.
    if [[ "${SOJU_EPIC_OVERLAY:-0}" != 1 ]]; then
      export WINEDLLOVERRIDES="${WINEDLLOVERRIDES:+$WINEDLLOVERRIDES;}EOSOVH-Win64-Shipping,EOSOVH-Win32-Shipping=d"
    fi
    EPIC="C:\\Program Files\\Epic Games\\Launcher\\Portal\\Binaries\\Win64\\EpicGamesLauncher.exe"
    [[ -f "$WINEPREFIX/drive_c/Program Files/Epic Games/Launcher/Portal/Binaries/Win64/EpicGamesLauncher.exe" ]] || \
      { echo "Epic Games Launcher not found, run scripts/create-epic-bottle.sh first"; exit 1; }
    pgrep -f "soju-reaper.sh $WINEPREFIX" >/dev/null 2>&1 || \
      { [ -x "$REAPER" ] && ( "$REAPER" "$WINEPREFIX" "$ENGINE/bin/wineserver" epic >/dev/null 2>&1 & ); }
    exec "$ENGINE/bin/wine" "$EPIC" "${@:2}"
    ;;
  epic-kill)   # Stop everything in the Epic bottle
    pkill -f "soju-reaper.sh $WINEPREFIX" 2>/dev/null || true
    "$ENGINE/bin/wineserver" -k 2>/dev/null || true
    sleep 2; [ -x "$SWEEP" ] && "$SWEEP"
    ;;
  gog)         # GOG GALAXY, same engine and env as Battle.net (verified 2026-08-30)
    # GOG GALAXY 2.x is Qt6 + QtWebEngine. Its D3D11 compositing path needs
    # IDXGIResource, which D3DMetal's DXGI does not implement, so the window
    # stays black unless Chromium runs on the CPU (--disable-gpu). GOG
    # overwrites QTWEBENGINE_CHROMIUM_FLAGS itself and ignores its argv, so the
    # engine's kernelbase/ucrtbase hook appends SOJU_CHROMIUM_FLAGS to that
    # variable whenever the program sets it (patches/chromium-flags-append.patch).
    # Closing the window parks GOG in the tray (macOS menu bar icon).
    GOG="C:\\Program Files\\GOG Galaxy\\GalaxyClient.exe"
    [[ -f "$WINEPREFIX/drive_c/Program Files/GOG Galaxy/GalaxyClient.exe" ]] || \
      { echo "GOG GALAXY not found, run scripts/create-gog-bottle.sh first"; exit 1; }
    rm -f "$WINEPREFIX/drive_c/ProgramData/GOG.com/Galaxy/lock-files/"* 2>/dev/null || true
    pgrep -f "soju-reaper.sh $WINEPREFIX" >/dev/null 2>&1 || \
      { [ -x "$REAPER" ] && ( "$REAPER" "$WINEPREFIX" "$ENGINE/bin/wineserver" gog >/dev/null 2>&1 & ); }
    export SOJU_CHROMIUM_FLAGS="${SOJU_CHROMIUM_FLAGS:---disable-gpu --disable-gpu-compositing}"
    export WINE_NO_DOCK_ICON="QtWebEngineProcess.exe;GalaxyClientService.exe;GOG Galaxy Notifications Renderer.exe;GalaxyCommunication.exe;GalaxyClientHelper.exe"
    # GOG draws its own title bar inside the client area; a macOS title bar on
    # top would hide the first rows of its UI (winemac patch: WINE_CUSTOM_FRAME).
    export WINE_CUSTOM_FRAME="GalaxyClient.exe"
    # Closing the window parks GOG in the tray. A Dock-icon click (winemac
    # patch: WINE_DOCK_REOPEN_CMD runs when no window is visible) sends the
    # same WM_COPYDATA "restore" message a second GalaxyClient.exe instance
    # would send, without paying for a whole second client start-up
    # (tools/soju-gog-restore.c, built by create-gog-bottle.sh).
    export WINE_DOCK_REOPEN_CMD="'$ENGINE/bin/wine' '${SOJU_BASE:-$HOME/.battlenet-macos}/gog-support/soju-gog-restore.exe'"
    exec python3 "$(dirname "${BASH_SOURCE[0]}")/gog-launch.py" "$ENGINE/bin/wine" "$GOG" "${@:2}"
    ;;
  gog-kill)    # Stop everything in the GOG bottle
    pkill -f "soju-reaper.sh $WINEPREFIX" 2>/dev/null || true
    "$ENGINE/bin/wineserver" -k 2>/dev/null || true
    sleep 2; [ -x "$SWEEP" ] && "$SWEEP"
    ;;
  steam)       # Steam client, wine-stable + webhelper wrapper (verified 2026-08-27)
    # Steam's CEF does not render on the CX engine (black screen / SEGV storm).
    # The verified combination is Homebrew wine-stable 11 + the steamwebhelper
    # wrapper (forces --disable-gpu --single-process) + -no-cef-sandbox
    # -cef-single-process. Source: github.com/notpop/steam-on-m1-wine (MIT),
    # vendored in third_party/.
    WINESTABLE="$STEAM_WINE_ROOT/bin/wine"
    WINESTABLE_SERVER="$STEAM_WINE_ROOT/bin/wineserver"
    [[ -x "$WINESTABLE" ]] || { echo "Steam runtime not found. Reinstall Steam from Platforms or run: soju steam-install"; exit 1; }
    ST="$WINEPREFIX/drive_c/Program Files (x86)/Steam/steam.exe"
    [[ -f "$ST" ]] || { echo "Steam not found, run scripts/create-steam-bottle.sh first"; exit 1; }
    # Repair a bootstrap-only installation before deploying the CEF wrapper.
    python3 "$(dirname "${BASH_SOURCE[0]}")/bootstrap-steam.py" "$WINESTABLE"
    # 1) Clean crash leftovers (a stale lock makes the next launch a windowless --silent one)
    find "$WINEPREFIX/drive_c/users/"*/AppData/Local/Steam/htmlcache -maxdepth 2 \
      \( -name "Singleton*" -o -name "*.lock" \) -delete 2>/dev/null || true
    # 2) Redeploy the wrapper (Steam updates restore the original, so check every launch)
    WRAP="$STEAM_SUPPORT/steamwebhelper-wrapper.exe"
    for d in "$WINEPREFIX/drive_c/Program Files (x86)/Steam/bin/cef"/cef.win*; do
      [[ -f "$d/steamwebhelper.exe" ]] || continue
      if [[ $(stat -f%z "$d/steamwebhelper.exe") -gt 500000 ]]; then
        mv -f "$d/steamwebhelper.exe" "$d/steamwebhelper_real.exe"
        cp -f "$WRAP" "$d/steamwebhelper.exe"
      fi
    done
    # 3) Keep virtual-desktop windows movable (one-time registry setting).
    #    Done before anything boots the prefix: a live wineserver rewrites
    #    user.reg on exit and would drop a line appended underneath it.
    if ! pgrep -x wineserver >/dev/null 2>&1; then
      grep -q 'AllowImmovableWindows' "$WINEPREFIX/user.reg" 2>/dev/null || {
        printf '\n[Software\\\\Wine\\\\Mac Driver]\n"AllowImmovableWindows"="n"\n' >> "$WINEPREFIX/user.reg"
      }
    fi
    # Everything that touches the Steam prefix runs without the CX engine
    # environment (wine-stable has its own dylibs and no msync), the first
    # process included: it is the one that boots the wineserver.
    STEAM_ENV=(env -u DYLD_FALLBACK_LIBRARY_PATH -u CX_ACTIVE_GRAPHICS_BACKEND -u CX_GRAPHICS_BACKEND
               -u CX_APPLEGPTK_LIBD3DSHARED_PATH -u WINEMSYNC -u ROSETTA_ADVERTISE_AVX -u WINE_SIMULATE_WRITECOPY
               WINEPREFIX="$WINEPREFIX" WINEDEBUG="${WINEDEBUG:-fixme-all}")
    # 4) Drop Steam's autostart entry. Steam adds itself to the prefix's Run key
    #    with -silent, so it comes back invisibly whenever anything boots this
    #    prefix. Steam re-adds it on update, so scrub it every launch.
    "${STEAM_ENV[@]}" "$WINESTABLE" reg delete 'HKCU\Software\Microsoft\Windows\CurrentVersion\Run' \
      /v Steam /f >/dev/null 2>&1 || true
    # 5) Virtual desktop: this macdrv (gcenx wine 11) ignores virtual desktops,
    #    so it is off by default. The single Dock icon is handled by
    #    WINE_NO_DOCK_ICON (winemac patch) instead.
    #    To enable anyway: WINE_VIRTUAL_DESKTOP=auto (or a resolution like 1600x900).
    VD="${WINE_VIRTUAL_DESKTOP-}"
    if [[ "$VD" == "auto" ]]; then
      B=$(osascript -e 'tell application "Finder" to get bounds of window of desktop' 2>/dev/null || true)
      if [[ "$B" =~ ,[[:space:]]*([0-9]+),[[:space:]]*([0-9]+)$ ]]; then
        VD="${BASH_REMATCH[1]}x${BASH_REMATCH[2]}"
      else
        VD="1728x1080"
      fi
    fi
    # 6) Launch: plain wine-stable environment, without the CX engine env
    STEAM_CMD=("C:\\Program Files (x86)\\Steam\\steam.exe" -no-cef-sandbox -cef-single-process)
    # SteamSetup only contains the bootstrapper: let first launch download the
    # client, otherwise Steam fails with "Failed to load steamui.dll".
    if [ -f "$WINEPREFIX/drive_c/Program Files (x86)/Steam/steamui.dll" ]; then
      STEAM_CMD+=(-noverifyfiles)
    fi
    STEAM_CMD+=("${@:2}")
    [[ -n "$VD" ]] && STEAM_CMD=(explorer.exe "/desktop=soju-steam,$VD" "${STEAM_CMD[@]}")
    # Closing the Steam window parks it in a tray macOS does not show; the patched
    # winemac runs WINE_DOCK_REOPEN_CMD on a Dock-icon click when no window is
    # visible, which makes the running Steam show its window again (same UX as
    # Windows' tray icon).
    # Reaper in steam mode: once Steam itself exits (Steam menu > Exit), tear the
    # rest of the bottle down so no Dock icon / winedevice lingers.
    pgrep -f "soju-reaper.sh $WINEPREFIX" >/dev/null 2>&1 || \
      { [ -x "$REAPER" ] && ( "$REAPER" "$WINEPREFIX" "$WINESTABLE_SERVER" steam >/dev/null 2>&1 & ); }
    "${STEAM_ENV[@]}" \
      WINEDLLOVERRIDES="bcrypt=b;ncrypt=b;gameoverlayrenderer,gameoverlayrenderer64=d" \
      WINE_NO_DOCK_ICON="steam.exe;steamservice.exe" \
      WINE_DOCK_REOPEN_CMD="'$WINESTABLE' 'C:\\Program Files (x86)\\Steam\\steam.exe' steam://open/main" \
      "$WINESTABLE" "${STEAM_CMD[@]}"
    ;;
  kill)        # Stop everything in the Battle.net bottle
    pkill -f "soju-reaper.sh $WINEPREFIX" 2>/dev/null || true
    "$ENGINE/bin/wineserver" -k 2>/dev/null || true
    sleep 2; [ -x "$SWEEP" ] && "$SWEEP"
    ;;
  steam-kill)  # Stop everything in the Steam bottle, note this bottle runs on
    # wine-stable, so it needs wine-stable's wineserver. Killing the CX engine's
    # wineserver here does nothing and leaves steam.exe alive, which then keeps
    # respawning steamwebhelper (it looks like "Steam relaunches itself").
    pkill -f "soju-reaper.sh $WINEPREFIX" 2>/dev/null || true
    "$STEAM_WINE_ROOT/bin/wineserver" -k 2>/dev/null || true
    sleep 2; [ -x "$SWEEP" ] && "$SWEEP"
    ;;
  *) echo "usage: play.sh [battlenet|d2r|epic|epic-kill|gog|gog-kill|steam|kill|steam-kill]"; exit 1;;
esac
