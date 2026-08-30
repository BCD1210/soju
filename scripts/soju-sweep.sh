#!/usr/bin/env bash
# soju-sweep: remove orphaned Wine service processes.
#
# Every bottle start spawns a set of idle Windows "system" processes
# (services.exe, winedevice.exe x2, plugplay.exe, rpcss.exe, explorer.exe
# /desktop: ~100 MB together). When the bottle's wineserver is killed
# (wineserver -k, a crashed launcher, an aborted test) the apps die, but these
# sleeping services never talk to the server again and so never notice it is
# gone; they linger for hours. This sweeps them: but ONLY when no wineserver
# is running at all, i.e. when nothing in any bottle can still be alive, so a
# running game or launcher is never touched.
#
# Usage: soju-sweep.sh        (called by play.sh *-kill and by soju-reaper.sh)
set -u
if pgrep -x wineserver >/dev/null 2>&1 || pgrep -x wineserver64 >/dev/null 2>&1; then
  exit 0   # something is live, do nothing
fi
# Also Wine's own prompts (the "Wine Mono Installer" dialog is control.exe
# appwiz.cpl install_mono): orphaned, they sit in the Dock as "wine" for hours.
RE='(explorer\.exe /desktop|services\.exe|winedevice\.exe|plugplay\.exe|rpcss\.exe|svchost\.exe|conhost\.exe|tabtip\.exe|control\.exe appwiz\.cpl|wineboot\.exe|winedbg\.exe)'
PIDS=$(pgrep -f "$RE" 2>/dev/null || true)
[ -n "$PIDS" ] || exit 0
echo "soju-sweep: removing orphaned Wine services: $(echo $PIDS | wc -w | tr -d ' ') processes"
kill -9 $PIDS 2>/dev/null || true
