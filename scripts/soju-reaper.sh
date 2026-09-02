#!/usr/bin/env bash
# soju-reaper: cleans up game processes that outlive their windows.
#
# Why: under Wine, quitting D2R sometimes leaves D2R.exe alive with no window
# (a shutdown thread misses a wakeup). The user sees a dead Dock icon and a
# process holding gigabytes of RAM. This watchdog kills a game process once it
# has gone windowless for two consecutive checks, and when the bottle is fully
# idle (no game, no launcher window) it shuts the whole prefix down so nothing
# lingers after "quit".
#
# Window detection uses CGWindowListCopyWindowInfo via python3+ctypes: no
# Accessibility permission needed, nothing to install.
#
# Usage: soju-reaper.sh <WINEPREFIX> <wineserver-path> [battlenet|steam|epic]
#        (backgrounded by play.sh / the Battle.net.app launcher)
#
# steam mode: closing the Steam window parks it in a tray (as on Windows; the
# Dock icon brings it back via WINE_DOCK_REOPEN_CMD). Once steam.exe itself has
# exited, the leftovers (steamservice, winedevice, wineserver) are shut down.
# epic mode: same tray semantics, keyed on EpicGamesLauncher.exe.
set -u

PREFIX="${1:?usage: soju-reaper.sh <WINEPREFIX> <wineserver> [battlenet|steam|epic]}"
WINESERVER="${2:?}"
MODE="${3:-battlenet}"
INTERVAL=20
SWEEP="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/soju-sweep.sh"
case "$MODE" in
  steam)
    GAME_RE='Steam\\steamapps\\'                      # games live under steamapps
    UI_RE='steam\.exe|steamwebhelper'
    TRAY_RE='steam\.exe'; TRAY_KILL_RE="$UI_RE|steamservice\.exe"
    ;;
  epic)
    GAME_RE='__none__'
    UI_RE='EpicGamesLauncher\.exe|EpicWebHelper\.exe|EpicOnlineServices'
    TRAY_RE='EpicGamesLauncher\.exe'; TRAY_KILL_RE="$UI_RE"
    ;;
  gog)
    GAME_RE='__none__'
    UI_RE='GalaxyClient\.exe|QtWebEngineProcess\.exe|GalaxyClientService\.exe|GOG Galaxy Notifications|GalaxyCommunication'
    TRAY_RE='GalaxyClient\.exe'; TRAY_KILL_RE="$UI_RE"
    ;;
  *)
    GAME_RE='D2R\.exe|BlizzardError\.exe'
    UI_RE='Battle\.net\.exe.*--from-launcher|Battle\.net Launcher\.exe'
    ;;
esac

# The wineserver for this prefix: wine names its socket directory after the
# prefix's device and inode, and the server runs with that directory as its
# working directory.  If the server is gone while the launcher is still up, the
# launcher is an orphan: every call that needs the server (closing a handle, for
# one) blocks forever, so its window stops repainting and stops responding while
# the process still looks alive.  Nothing can rescue it at that point, and the
# leftovers have to be killed for the next start to work.
# lsof reports the resolved path, and on macOS /tmp is a symlink to
# /private/tmp, so compare against the resolved form as well: with only the
# /tmp spelling the check never matched, and the reaper killed every launcher
# 40 s after it started.
SERVER_DIR="$(/usr/bin/stat -f "/tmp/.wine-$(id -u)/server-%Xd-%Xi" "$PREFIX" 2>/dev/null || true)"
SERVER_DIR_REAL="$(cd /tmp 2>/dev/null && pwd -P)${SERVER_DIR#/tmp}"

server_alive() {
  local pids
  [ -n "$SERVER_DIR" ] || return 0     # cannot tell, assume it is there
  pids=$(pgrep -x wineserver 2>/dev/null | tr '\n' ',' | sed 's/,$//')
  [ -n "$pids" ] || return 1
  lsof -a -p "$pids" -d cwd -Fn 2>/dev/null | grep -qxF -e "n$SERVER_DIR" -e "n$SERVER_DIR_REAL"
}

reap_orphans() {
  echo "soju-reaper: the wineserver for $PREFIX is gone, cleaning up its leftovers"
  pkill -9 -f "${TRAY_KILL_RE:-$UI_RE}" 2>/dev/null || true
  [ "$GAME_RE" != "__none__" ] && pkill -9 -f "$GAME_RE" 2>/dev/null || true
  sleep 2; [ -x "$SWEEP" ] && "$SWEEP" >/dev/null 2>&1
}

window_pids() {
  /usr/bin/python3 - <<'PY'
import ctypes
cg = ctypes.CDLL("/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics")
cf = ctypes.CDLL("/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation")
cg.CGWindowListCopyWindowInfo.restype = ctypes.c_void_p
cg.CGWindowListCopyWindowInfo.argtypes = [ctypes.c_uint32, ctypes.c_uint32]
cf.CFArrayGetCount.restype = ctypes.c_long
cf.CFArrayGetCount.argtypes = [ctypes.c_void_p]
cf.CFArrayGetValueAtIndex.restype = ctypes.c_void_p
cf.CFArrayGetValueAtIndex.argtypes = [ctypes.c_void_p, ctypes.c_long]
cf.CFDictionaryGetValue.restype = ctypes.c_void_p
cf.CFDictionaryGetValue.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
cf.CFStringCreateWithCString.restype = ctypes.c_void_p
cf.CFStringCreateWithCString.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_uint32]
cf.CFNumberGetValue.restype = ctypes.c_bool
cf.CFNumberGetValue.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_void_p]
key = cf.CFStringCreateWithCString(None, b"kCGWindowOwnerPID", 0x08000100)
arr = cg.CGWindowListCopyWindowInfo(1, 0)  # kCGWindowListOptionOnScreenOnly
pids = set()
for i in range(cf.CFArrayGetCount(arr)):
    num = cf.CFDictionaryGetValue(cf.CFArrayGetValueAtIndex(arr, i), key)
    if num:
        v = ctypes.c_int(0)
        if cf.CFNumberGetValue(num, 9, ctypes.byref(v)):
            pids.add(v.value)
print(" ".join(map(str, sorted(pids))))
PY
}

declare -A STRIKES
IDLE_STRIKES=0
ORPHAN_STRIKES=0

if [ "$MODE" = "steam" ] || [ "$MODE" = "epic" ] || [ "$MODE" = "gog" ]; then
  # Steam parks itself in an (invisible on macOS) tray when its window is closed,
  # so a missing window means nothing here. Only once steam.exe has really gone
  # (Steam menu > Exit) do we shut the rest of the bottle down.
  while true; do
    sleep "$INTERVAL"
    if ! server_alive; then
      ORPHAN_STRIKES=$((ORPHAN_STRIKES + 1))
      if [ "$ORPHAN_STRIKES" -ge 2 ]; then reap_orphans; exit 0; fi
    else
      ORPHAN_STRIKES=0
    fi
    if ! pgrep -f "$TRAY_RE" >/dev/null 2>&1; then
      IDLE_STRIKES=$((IDLE_STRIKES + 1))
      if [ "$IDLE_STRIKES" -ge 2 ]; then
        WINEPREFIX="$PREFIX" "$WINESERVER" -k 2>/dev/null || true
        sleep 3; [ -x "$SWEEP" ] && "$SWEEP" >/dev/null 2>&1
        sleep 3
        pkill -9 -f "$TRAY_KILL_RE" 2>/dev/null || true
        exit 0
      fi
    else
      IDLE_STRIKES=0
    fi
  done
fi

while true; do
  sleep "$INTERVAL"

  if ! server_alive; then
    ORPHAN_STRIKES=$((ORPHAN_STRIKES + 1))
    if [ "$ORPHAN_STRIKES" -ge 2 ]; then reap_orphans; exit 0; fi
  else
    ORPHAN_STRIKES=0
  fi

  GAME_PIDS=$(pgrep -f "$GAME_RE" 2>/dev/null || true)
  UI_PIDS=$(pgrep -f "$UI_RE" 2>/dev/null || true)

  # Bottle fully gone? Nothing left to do.
  if [ -z "$GAME_PIDS" ] && [ -z "$UI_PIDS" ]; then
    IDLE_STRIKES=$((IDLE_STRIKES + 1))
    # Two idle checks in a row: tear down leftover services (Agent, winedevice…)
    if [ "$IDLE_STRIKES" -ge 2 ]; then
      WINEPREFIX="$PREFIX" "$WINESERVER" -k 2>/dev/null || true
      sleep 3; [ -x "$SWEEP" ] && "$SWEEP" >/dev/null 2>&1
      exit 0
    fi
    continue
  fi
  IDLE_STRIKES=0

  [ -z "$GAME_PIDS" ] && continue
  WINS=" $(window_pids) "

  for pid in $GAME_PIDS; do
    if [[ "$WINS" == *" $pid "* ]]; then
      STRIKES[$pid]=0
    else
      STRIKES[$pid]=$(( ${STRIKES[$pid]:-0} + 1 ))
      # Windowless on two consecutive checks (>=40s): it's a zombie, reap it.
      if [ "${STRIKES[$pid]}" -ge 2 ]; then
        kill -9 "$pid" 2>/dev/null || true
        unset "STRIKES[$pid]"
      fi
    fi
  done
done
