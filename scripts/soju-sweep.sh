#!/usr/bin/env bash
# soju-sweep: remove orphaned Wine service processes.
#
# Every bottle start spawns a set of idle Windows "system" processes
# (services.exe, winedevice.exe x2, plugplay.exe, rpcss.exe, explorer.exe
# /desktop: ~100 MB together). When the bottle's wineserver is killed
# (wineserver -k, a crashed launcher, an aborted test) the apps die, but these
# sleeping services never talk to the server again and so never notice it is
# gone; they linger for hours. This sweeps them.
#
# Only service processes are candidates, never launchers or games, and a
# candidate is removed only if it is attached to no running wineserver: every
# process of a live bottle holds a file open in its server's socket directory
# (the server's working directory), so the ones without such a file belong to
# a server that is gone. Safe to run while other bottles are up.
#
# Usage: soju-sweep.sh        (called by play.sh *-kill, soju-reaper.sh, soju sweep)
#        SOJU_SWEEP_DRY=1 soju-sweep.sh    only print what would be removed
set -u
# Also Wine's own prompts (the "Wine Mono Installer" dialog is control.exe
# appwiz.cpl install_mono): orphaned, they sit in the Dock as "wine" for hours.
RE='(explorer\.exe /desktop|services\.exe|winedevice\.exe|plugplay\.exe|rpcss\.exe|svchost\.exe|conhost\.exe|tabtip\.exe|control\.exe appwiz\.cpl|wineboot\.exe|winedbg\.exe|start\.exe /exec)'
CAND=$(pgrep -f "$RE" 2>/dev/null || true)
[ -n "$CAND" ] || exit 0

SERVERS=$( { pgrep -x wineserver; pgrep -x wineserver64; } 2>/dev/null | tr '\n' ',' | sed 's/,$//')
ATTACHED=""
if [ -n "$SERVERS" ]; then
  # One line per server directory, joined with "|" (macOS awk rejects a
  # newline inside -v, and would silently see no directories at all).
  DIRS=$(lsof -a -p "$SERVERS" -d cwd -Fn 2>/dev/null | sed -n 's/^n//p' | tr '\n' '|')
  ATTACHED=$(lsof -a -p "$(echo "$CAND" | tr '\n' ',' | sed 's/,$//')" -Fpn 2>/dev/null | awk -v dirs="$DIRS" '
    BEGIN { n = split(dirs, d, "|") }
    /^p/ { pid = substr($0, 2) }
    /^n/ { for (i = 1; i <= n; i++) if (d[i] != "" && index($0, "n" d[i] "/") == 1) { seen[pid] = 1; break } }
    END  { for (p in seen) print p }' | tr '\n' ' ')
fi

PIDS=""
for p in $CAND; do
  case " $ATTACHED " in *" $p "*) ;; *) PIDS="$PIDS $p" ;; esac
done
[ -n "${PIDS// /}" ] || exit 0
if [ "${SOJU_SWEEP_DRY:-0}" = 1 ]; then
  echo "soju-sweep (dry run): would remove $(echo $PIDS | wc -w | tr -d ' ') orphaned Wine services:"
  for p in $PIDS; do echo "  $p $(ps -o args= -p "$p" 2>/dev/null | cut -c1-80)"; done
  exit 0
fi
echo "soju-sweep: removing orphaned Wine services: $(echo $PIDS | wc -w | tr -d ' ') processes"
# shellcheck disable=SC2086
kill -9 $PIDS 2>/dev/null || true
