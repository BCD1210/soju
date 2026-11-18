# Sourced by play.sh: find and stop Wine processes of one bottle that its wineserver no longer owns.
#
# A bottle's processes all keep files open under the prefix (drive_c/windows at least). When the
# wineserver dies first (a crash, or a kill that only reached the server), they become orphans:
# every call that needs the server blocks, windows stop answering, `wineserver -k` cannot reach
# them, and macOS cannot quit them for a logout or shutdown. The next launch then starts a second
# server and a second set of processes next to them (reported on Reddit: "the first bottle was
# impossible to stop and prevented switching off the computer").

# PIDs of Wine processes holding a file under the prefix, or a shared-memory file in the
# prefix's wineserver directory (/tmp/.wine-UID/server-DEV-INODE, named after the prefix; start.exe
# holds nothing else). Only processes whose first argument is a Windows program (C:\..., C:/...,
# start.exe) count, so a Terminal `tail -f` on a log inside the bottle is never touched.
bottle_pids() {   # prefix
  local cand real dir pid first out=""
  real="$(cd "$1" 2>/dev/null && pwd -P)" || return 0
  dir=$(/usr/bin/stat -L -f "/private/tmp/.wine-$(id -u)/server-%Xd-%Xi" "$real" 2>/dev/null) || dir="$real"
  cand=$(pgrep -f '\.exe' 2>/dev/null | tr '\n' ',' | sed 's/,$//')
  [ -n "$cand" ] || return 0
  for pid in $(lsof -a -p "$cand" -Fpn 2>/dev/null | awk -v P="n$real/" -v S="n$dir/" '
      /^p/ { pid = substr($0, 2) }
      /^n/ { if (index($0, P) == 1 || index($0, S) == 1) seen[pid] = 1 }
      END  { for (p in seen) print p }' | sort -un); do
    first=$(ps -o args= -p "$pid" 2>/dev/null | awk '{print $1}')
    case "$first" in /*|"") continue ;; esac
    out="$out $pid"
  done
  printf '%s\n' $out
}

# 0 if a wineserver serves this prefix, 1 if none does, 2 if that could not be read.
bottle_server_alive() {   # prefix
  local real dir pids cwd
  real="$(cd "$1" 2>/dev/null && pwd -P)" || return 2
  dir=$(/usr/bin/stat -L -f "/tmp/.wine-$(id -u)/server-%Xd-%Xi" "$real" 2>/dev/null) || return 2
  pids=$( { pgrep -x wineserver; pgrep -x wineserver64; } 2>/dev/null | tr '\n' ',' | sed 's/,$//')
  [ -n "$pids" ] || return 1
  cwd=$(lsof -a -p "$pids" -d cwd -Fn 2>/dev/null)
  [ -n "$cwd" ] || return 2
  printf '%s\n' "$cwd" | grep -qxF -e "n$dir" -e "n/private$dir" && return 0
  return 1
}

# Kill what is left of the bottle (after `wineserver -k`, or when the server is gone).
bottle_kill_leftovers() {   # prefix
  local pids
  pids=$(bottle_pids "$1")
  [ -n "${pids//[[:space:]]/}" ] || return 0
  # shellcheck disable=SC2086
  kill -9 $pids 2>/dev/null || true
  echo "soju: stopped $(printf '%s\n' $pids | wc -l | tr -d ' ') leftover process(es) in $(basename "$1")" >&2
}

# Before a launch: orphans without a server would sit next to the new session; clear them.
bottle_clear_orphans() {   # prefix (play.sh runs with set -e: no bare non-zero status here)
  local alive=0
  bottle_server_alive "$1" || alive=$?
  [ "$alive" -eq 1 ] || return 0
  bottle_kill_leftovers "$1" || true
}
