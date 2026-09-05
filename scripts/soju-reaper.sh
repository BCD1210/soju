#!/usr/bin/env bash
# soju-reaper: cleans up game processes that outlive their windows, and shuts a
# bottle down once nothing but its idle services is left in it.
#
# Why: under Wine, quitting D2R sometimes leaves D2R.exe alive with no window
# (a shutdown thread misses a wakeup). The user sees a dead Dock icon and a
# process holding gigabytes of RAM. This watchdog kills a game process once it
# has gone windowless for three consecutive checks, and when the bottle is
# fully idle it shuts the whole prefix down so nothing lingers after "quit".
#
# Which processes belong to this bottle: every Wine process, builtin services
# included, keeps a shared-memory file open in its wineserver's socket
# directory (/tmp/.wine-UID/server-DEV-INODE/tmpmap-*), and that directory is
# named after the prefix. `lsof +d` on it lists exactly this bottle's
# processes in well under 100 ms, so nothing here matches on process names
# alone: a `tail -f D2R.exe.log` in Terminal, or the same game running from
# another bottle, is never touched.
#
# "Idle" means: no process in the bottle except the Windows service set and
# the launcher's own helpers (Agent.exe for Battle.net, EOS helpers for Epic,
# GalaxyClientService for GOG, steamservice for Steam). A launcher parked in
# its tray, a game started from a launcher that has since been quit, or a
# launcher self-update running msiexec all keep the bottle alive.
#
# Window detection uses CGWindowListCopyWindowInfo via python3+ctypes: no
# Accessibility permission needed, nothing to install. A minimized or hidden
# (Cmd-H) window, or one on another Space, still counts as a window.
#
# Usage: soju-reaper.sh <WINEPREFIX> <wineserver-path> [battlenet|steam|epic|gog]
#        (backgrounded by play.sh / the app bundles)
set -u

PREFIX="${1:?usage: soju-reaper.sh <WINEPREFIX> <wineserver> [battlenet|steam|epic|gog]}"
WINESERVER="${2:?}"
MODE="${3:-battlenet}"
INTERVAL=20
ZOMBIE_STRIKES=3       # windowless checks before a game process is killed (60 s)
SWEEP="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/soju-sweep.sh"

# Basenames are matched against the first argv word of a Wine process
# ("C:\...\D2R.exe -launch ..."), anchored on the path separator and the end of
# that word, so "D2R.exe.log" or "steam.exe.bak" do not match.
exe_re(){ local alt="$1"; printf '(^|[\\/])(%s)( |$)' "$alt"; }

# Idle Windows service set (also what soju-sweep removes once no server is up).
SERVICE_RE="$(exe_re 'services\.exe|winedevice\.exe|plugplay\.exe|rpcss\.exe|svchost\.exe|explorer\.exe|conhost\.exe|tabtip\.exe|wineboot\.exe|winedbg\.exe|start\.exe|control\.exe')"

case "$MODE" in
  steam)
    GAME_RE=''                                             # games keep their windows on wine-stable
    HELPER_RE="$(exe_re 'steamservice\.exe|steamwebhelper\.exe|steamwebhelper_real\.exe|steamerrorreporter\.exe|steamerrorreporter64\.exe')"
    ;;
  epic)
    GAME_RE=''
    HELPER_RE="$(exe_re 'EpicOnlineServicesUserHelper\.exe|EpicOnlineServicesHost\.exe|EpicOnlineServicesUIHelper\.exe|EpicOnlineServicesInstallHelper\.exe|EOSOverlayRenderer-Win64-Shipping\.exe|EOSOverlayRenderer-Win32-Shipping\.exe|EpicWebHelper\.exe|CrashReportClient\.exe')"
    ;;
  gog)
    GAME_RE=''
    HELPER_RE="$(exe_re 'GalaxyClientService\.exe|GalaxyCommunication\.exe|GalaxyClientHelper\.exe|GOG Galaxy Notifications Renderer\.exe|QtWebEngineProcess\.exe|GalaxyClient Helper\.exe')"
    ;;
  *)
    GAME_RE="$(exe_re 'D2R\.exe')"
    HELPER_RE="$(exe_re 'Agent\.exe|Battle\.net Helper\.exe|Battle\.net Update Agent\.exe|BlizzardError\.exe|SystemSurvey\.exe')"
    ;;
esac

# The wineserver for this prefix: wine names its socket directory after the
# prefix's device and inode, and the server runs with that directory as its
# working directory. If the server is gone while the launcher is still up, the
# launcher is an orphan: every call that needs the server (closing a handle,
# for one) blocks forever, so its window stops repainting and stops responding
# while the process still looks alive. Nothing can rescue it at that point, and
# the leftovers have to be killed for the next start to work.
#
# lsof reports the resolved path, and on macOS /tmp is a symlink to
# /private/tmp, so compare against the resolved form as well: with only the
# /tmp spelling the check never matched, and the reaper killed every launcher
# 40 s after it started.
#
# Wine names the directory after the *resolved* prefix. A prefix that is a
# symlink (a bottle kept in another app's container, say) must be resolved
# first: macOS stat uses lstat by default, and the symlink's own inode never
# matches, so the reaper would count the live server as gone and reap the
# bottle 40 s after every start.
PREFIX_REAL="$(cd "$PREFIX" 2>/dev/null && pwd -P)"
SERVER_DIR="$(/usr/bin/stat -L -f "/tmp/.wine-$(id -u)/server-%Xd-%Xi" "${PREFIX_REAL:-$PREFIX}" 2>/dev/null || true)"
SERVER_DIR_REAL="$(cd /tmp 2>/dev/null && pwd -P)${SERVER_DIR#/tmp}"

# Returns 0 alive, 1 gone, 2 unknown (the query itself failed: every
# wineserver has a cwd, so no output at all means lsof did not answer, and
# the caller must skip the round rather than count an orphan strike).
server_alive() {
  local pids out
  [ -n "$SERVER_DIR" ] || return 0     # cannot tell, assume it is there
  pids=$( { pgrep -x wineserver; pgrep -x wineserver64; } 2>/dev/null | tr '\n' ',' | sed 's/,$//')
  [ -n "$pids" ] || return 1
  out=$(lsof -a -p "$pids" -d cwd -Fn 2>/dev/null)
  [ -n "$out" ] || return 2
  printf '%s\n' "$out" | grep -qxF -e "n$SERVER_DIR" -e "n$SERVER_DIR_REAL"
}

# PIDs of this bottle's Wine processes (the server itself excluded): every one
# of them holds a file open in the server directory.
bottle_pids() {
  local cand
  [ -n "$SERVER_DIR" ] || return 0
  cand=$(pgrep -f '\.exe' 2>/dev/null | tr '\n' ',' | sed 's/,$//')
  [ -n "$cand" ] || return 0
  lsof -a -p "$cand" +d "$SERVER_DIR_REAL" -Fp 2>/dev/null | sed -n 's/^p//p' | sort -un
}

# First argv word of a process, i.e. its Windows exe path.
exe_of() { ps -o args= -p "$1" 2>/dev/null | sed 's/ -.*//; s/ \/.*//'; }

# Split a pid list into what keeps the bottle alive and what does not.
classify() {   # sets ALIVE_PIDS, GAME_PIDS, HELPER_PIDS
  local pid exe
  ALIVE_PIDS=""; GAME_PIDS=""; HELPER_PIDS=""
  for pid in $1; do
    exe=$(ps -o args= -p "$pid" 2>/dev/null) || continue
    [[ "$exe" =~ $SERVICE_RE ]] && continue
    [[ "$exe" =~ $HELPER_RE ]] && { HELPER_PIDS="$HELPER_PIDS $pid"; continue; }
    ALIVE_PIDS="$ALIVE_PIDS $pid"
    [ -n "$GAME_RE" ] && [[ "$exe" =~ $GAME_RE ]] && GAME_PIDS="$GAME_PIDS $pid"
  done
}

# PIDs of Wine processes holding any file open under this prefix: every
# process of a bottle keeps at least drive_c/windows open, so this finds them
# without the server (which the orphan case has lost).
prefix_pids() {
  local cand real
  cand=$(pgrep -f '\.exe' 2>/dev/null | tr '\n' ',' | sed 's/,$//')
  [ -n "$cand" ] || return 0
  real="$(cd "$PREFIX" 2>/dev/null && pwd -P)" || return 0
  lsof -a -p "$cand" -Fpn 2>/dev/null | awk -v P="n$real/" '
    /^p/ { pid = substr($0, 2) }
    /^n/ { if (index($0, P) == 1) seen[pid] = 1 }
    END  { for (p in seen) print p }' | sort -un
}

reap_orphans() {
  local pids
  echo "soju-reaper: the wineserver for $PREFIX is gone, cleaning up its leftovers"
  pids=$(prefix_pids)
  # shellcheck disable=SC2086
  [ -n "${pids// /}" ] && kill -9 $pids 2>/dev/null || true
  sleep 2; [ -x "$SWEEP" ] && "$SWEEP" >/dev/null 2>&1
}

# PIDs that own at least one real window: on-screen, or off-screen but a
# normal-layer window with a size (minimized, hidden, on another Space). Wine
# keeps 1x1 fully transparent placeholder windows for every process, which do
# not count. Prints nothing (and the caller skips the round) if the query fails.
window_pids() {
  /usr/bin/python3 - <<'PY'
import ctypes, sys
try:
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
    cf.CFBooleanGetValue.restype = ctypes.c_bool
    cf.CFBooleanGetValue.argtypes = [ctypes.c_void_p]
    def key(s): return cf.CFStringCreateWithCString(None, s, 0x08000100)
    kPID, kLayer, kAlpha, kOn, kBounds = key(b"kCGWindowOwnerPID"), key(b"kCGWindowLayer"), key(b"kCGWindowAlpha"), key(b"kCGWindowIsOnscreen"), key(b"kCGWindowBounds")
    kW, kH = key(b"Width"), key(b"Height")
    def num(d, k, kind=9, ctype=ctypes.c_int):
        n = cf.CFDictionaryGetValue(d, k)
        if not n: return None
        v = ctype(0)
        return v.value if cf.CFNumberGetValue(n, kind, ctypes.byref(v)) else None
    arr = cg.CGWindowListCopyWindowInfo(0, 0)   # kCGWindowListOptionAll
    pids = set()
    for i in range(cf.CFArrayGetCount(arr)):
        w = cf.CFArrayGetValueAtIndex(arr, i)
        pid = num(w, kPID)
        if pid is None: continue
        # Placeholder filter first, for on-screen and off-screen windows alike:
        # Wine's 1x1 fully transparent windows are never a real window.
        alpha = num(w, kAlpha, 13, ctypes.c_double)
        if alpha is not None and alpha <= 0: continue
        b = cf.CFDictionaryGetValue(w, kBounds)
        if not b: continue
        wd, ht = num(b, kW, 13, ctypes.c_double), num(b, kH, 13, ctypes.c_double)
        if wd is None or ht is None or wd <= 1 or ht <= 1: continue
        on = cf.CFDictionaryGetValue(w, kOn)
        if on and cf.CFBooleanGetValue(on):
            pids.add(pid); continue          # any layer: a full screen game sits above layer 0
        if num(w, kLayer) != 0: continue     # off-screen: only normal windows (minimized, hidden, other Space)
        pids.add(pid)
    print(" ".join(map(str, sorted(pids))))
except Exception:
    sys.exit(1)
PY
}

declare -a STRIKES SEEN_WINDOWS GAME_IDENTITIES   # indexed by pid
IDLE_STRIKES=0
ORPHAN_STRIKES=0

while true; do
  sleep "$INTERVAL"

  server_alive; alive=$?
  [ "$alive" -ne 2 ] || continue        # query failed: not evidence of anything
  if [ "$alive" -ne 0 ]; then
    ORPHAN_STRIKES=$((ORPHAN_STRIKES + 1))
    if [ "$ORPHAN_STRIKES" -ge 2 ]; then reap_orphans; exit 0; fi
    continue
  fi
  ORPHAN_STRIKES=0

  # A live server always has its service processes attached, so an empty
  # answer here is a failed attribution (lsof hiccup), not an empty bottle:
  # skip the round rather than read it as idle and take the bottle down.
  BOTTLE=$(bottle_pids)
  [ -n "${BOTTLE// /}" ] || continue
  classify "$BOTTLE"

  # Nothing but services and helpers left? Two checks in a row and the bottle
  # comes down (wineserver -k), then the sweep removes the service set.
  if [ -z "${ALIVE_PIDS// /}" ]; then
    IDLE_STRIKES=$((IDLE_STRIKES + 1))
    if [ "$IDLE_STRIKES" -ge 2 ]; then
      WINEPREFIX="$PREFIX" "$WINESERVER" -k 2>/dev/null || true
      sleep 3; [ -x "$SWEEP" ] && "$SWEEP" >/dev/null 2>&1
      sleep 3
      # Helpers do not always notice the server going away. Only this
      # bottle's (from the classification above), never another bottle's.
      # shellcheck disable=SC2086
      [ -n "${HELPER_PIDS// /}" ] && kill -9 $HELPER_PIDS 2>/dev/null || true
      exit 0
    fi
    continue
  fi
  IDLE_STRIKES=0

  # A cold start may load for minutes before creating its first window.
  # Reap only after a previously observed window disappears. Start time
  # prevents a reused PID from inheriting another game's window history.
  [ -n "${GAME_PIDS// /}" ] || continue
  WINS=" $(window_pids) " || continue
  [ "$WINS" != "  " ] || continue     # query failed or no windows anywhere: skip the round
  for pid in "${!GAME_IDENTITIES[@]}"; do
    case " $GAME_PIDS " in *" $pid "*) ;; *)
      unset "GAME_IDENTITIES[$pid]" "SEEN_WINDOWS[$pid]" "STRIKES[$pid]" ;;
    esac
  done
  for pid in $GAME_PIDS; do
    identity=$(ps -o lstart= -p "$pid" 2>/dev/null) || continue
    [ -n "$identity" ] || continue
    if [ "${GAME_IDENTITIES[$pid]:-}" != "$identity" ]; then
      GAME_IDENTITIES[$pid]="$identity"
      SEEN_WINDOWS[$pid]=0
      STRIKES[$pid]=0
    fi
    if [[ "$WINS" == *" $pid "* ]]; then
      SEEN_WINDOWS[$pid]=1
      STRIKES[$pid]=0
    elif [ "${SEEN_WINDOWS[$pid]:-0}" = 1 ]; then
      STRIKES[$pid]=$(( ${STRIKES[$pid]:-0} + 1 ))
      if [ "${STRIKES[$pid]}" -ge "$ZOMBIE_STRIKES" ]; then
        echo "soju-reaper: $(exe_of "$pid") ($pid) has had no window for $((ZOMBIE_STRIKES * INTERVAL)) s, killing it"
        kill -9 "$pid" 2>/dev/null || true
        unset "STRIKES[$pid]"
      fi
    fi
  done
done
