#!/usr/bin/env bash
# soju-reaper — cleans up game processes that outlive their windows.
#
# Why: under Wine, quitting D2R sometimes leaves D2R.exe alive with no window
# (a shutdown thread misses a wakeup). The user sees a dead Dock icon and a
# process holding gigabytes of RAM. This watchdog kills a game process once it
# has gone windowless for two consecutive checks, and when the bottle is fully
# idle (no game, no launcher window) it shuts the whole prefix down so nothing
# lingers after "quit".
#
# Window detection uses CGWindowListCopyWindowInfo via python3+ctypes — no
# Accessibility permission needed, nothing to install.
#
# Usage: soju-reaper.sh <WINEPREFIX> <wineserver-path> [battlenet|steam]
#        (backgrounded by play.sh / the Battle.net.app launcher)
#
# steam mode: the Windows Steam client never really quits — closing its window
# just parks it in a tray that macOS does not show, so the Dock icon and half a
# dozen processes linger. In steam mode "no Steam/game window on screen for two
# checks" means the user is done: the whole Steam bottle is shut down.
set -u

PREFIX="${1:?usage: soju-reaper.sh <WINEPREFIX> <wineserver> [battlenet|steam]}"
WINESERVER="${2:?}"
MODE="${3:-battlenet}"
INTERVAL=20
case "$MODE" in
  steam)
    GAME_RE='Steam\\steamapps\\'                      # games live under steamapps
    UI_RE='steam\.exe|steamwebhelper'
    ;;
  *)
    GAME_RE='D2R\.exe|BlizzardError\.exe'
    UI_RE='Battle\.net\.exe.*--from-launcher|Battle\.net Launcher\.exe'
    ;;
esac

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

if [ "$MODE" = "steam" ]; then
  # Wait for Steam to show a window first (login/update can take a while), then
  # tear the bottle down once neither Steam nor a game has a window for 40s.
  SEEN=0; NOWIN=0
  while true; do
    sleep "$INTERVAL"
    PIDS=$(pgrep -f "$UI_RE|$GAME_RE" 2>/dev/null || true)
    [ -z "$PIDS" ] && { [ "$SEEN" = 1 ] && exit 0; continue; }
    WINS=" $(window_pids) "
    HAS=0
    for pid in $PIDS; do [[ "$WINS" == *" $pid "* ]] && { HAS=1; break; }; done
    if [ "$HAS" = 1 ]; then SEEN=1; NOWIN=0; continue; fi
    [ "$SEEN" = 1 ] || continue
    NOWIN=$((NOWIN + 1))
    if [ "$NOWIN" -ge 2 ]; then
      WINEPREFIX="$PREFIX" "$WINESERVER" -k 2>/dev/null || true
      sleep 3
      pkill -9 -f "$UI_RE|steamservice\.exe" 2>/dev/null || true
      exit 0
    fi
  done
fi

while true; do
  sleep "$INTERVAL"

  GAME_PIDS=$(pgrep -f "$GAME_RE" 2>/dev/null || true)
  UI_PIDS=$(pgrep -f "$UI_RE" 2>/dev/null || true)

  # Bottle fully gone? Nothing left to do.
  if [ -z "$GAME_PIDS" ] && [ -z "$UI_PIDS" ]; then
    IDLE_STRIKES=$((IDLE_STRIKES + 1))
    # Two idle checks in a row: tear down leftover services (Agent, winedevice…)
    if [ "$IDLE_STRIKES" -ge 2 ]; then
      WINEPREFIX="$PREFIX" "$WINESERVER" -k 2>/dev/null || true
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
