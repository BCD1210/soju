#!/usr/bin/env bash
# Create a fresh bottle without CrossOver and install Battle.net with Blizzard's
# official installer.
# Verified 2026-08-27: automatic install through to the login screen on a pristine bottle.
set -euo pipefail

ENGINE="${ENGINE:-$HOME/.battlenet-macos/cx26-engine}"
export WINEPREFIX="${WINEPREFIX:-$HOME/.battlenet-macos/bottle}"
export WINEDEBUG="${WINEDEBUG:-fixme-all}"
export WINEMSYNC=1 ROSETTA_ADVERTISE_AVX=1 WINE_SIMULATE_WRITECOPY=1
export CX_APPLEGPTK_LIBD3DSHARED_PATH="$ENGINE/lib/external/libd3dshared.dylib"
export DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib"

[ -x "$ENGINE/bin/wine" ] || { echo "Engine not found, run build-engine.sh first"; exit 1; }
[ -f "$CX_APPLEGPTK_LIBD3DSHARED_PATH" ] || { echo "libd3dshared not found, run get-gptk.sh first"; exit 1; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORK="${WORK:-$HOME/.battlenet-macos/build}"; mkdir -p "$WORK"

echo "==> Initializing the prefix: $WINEPREFIX"
"$ENGINE/bin/wine" wineboot -u >/dev/null 2>&1 || true
"$ENGINE/bin/wineserver" -w

echo "==> Silencing crash dialogs (two harmless startup crashes would pop windows)"
# Battle.net harmlessly loses two helper threads at startup; by default each one
# pops an error dialog. Swap in a debugger that quietly freezes the thread
# (details: docs/DIAGNOSIS.md).
W="$ENGINE/bin/wine"
"$W" reg add "HKCU\\Software\\Wine\\WineDbg" /v ShowCrashDialog /t REG_DWORD /d 0 /f >/dev/null 2>&1
for HIVE in "HKLM\\Software\\Microsoft\\Windows NT\\CurrentVersion\\AeDebug" \
            "HKLM\\Software\\Wow6432Node\\Microsoft\\Windows NT\\CurrentVersion\\AeDebug"; do
  "$W" reg add "$HIVE" /v Debugger /t REG_SZ /d "C:\\windows\\system32\\rundll32.exe kernel32.dll,Sleep" /f >/dev/null 2>&1
  "$W" reg add "$HIVE" /v Auto /t REG_SZ /d 1 /f >/dev/null 2>&1
done
"$ENGINE/bin/wineserver" -w

BN="$WINEPREFIX/drive_c/Program Files (x86)/Battle.net"
PROGRAMDATA="$WINEPREFIX/drive_c/ProgramData"
SETUP="$WORK/Battle.net-Setup.exe"
SETUP_TIMEOUT="${SETUP_TIMEOUT:-300}"

download_setup() {
  [ -f "$SETUP" ] || { curl -fL "https://downloader.battle.net/download/getInstaller?os=win&installer=Battle.net-Setup.exe" -o "$SETUP.part" && mv -f "$SETUP.part" "$SETUP"; }
}

# Blizzard's setup resumes from what an unfinished attempt left in ProgramData
# and can stall at 45% on it. Without a finished install, move that state aside
# (kept for its logs) so the installer starts clean.
set_aside_setup_state() {
  [ ! -f "$BN/Battle.net.exe" ] || return 0
  local keep="" dir
  for dir in "Battle.net" "Battle.net_components" "Blizzard Entertainment"; do
    [ -e "$PROGRAMDATA/$dir" ] || continue
    [ -n "$keep" ] || keep=$(mktemp -d "$WORK/previous-setup-$(date +%Y%m%d-%H%M%S).XXXX")
    mv "$PROGRAMDATA/$dir" "$keep/"
  done
  [ -z "$keep" ] || echo "    Moved an unfinished earlier setup aside: $keep"
}

# Succeeds once the installer exits or Battle.net.exe exists; stops a stalled
# installer after SETUP_TIMEOUT seconds instead of waiting forever.
run_setup() {
  "$ENGINE/bin/wine" "$SETUP" --lang=enUS >/dev/null 2>&1 &
  local pid=$! waited=0
  while kill -0 "$pid" 2>/dev/null; do
    if [ "$waited" -ge "$SETUP_TIMEOUT" ]; then
      [ ! -f "$BN/Battle.net.exe" ] || return 0
      kill "$pid" 2>/dev/null || true
      "$ENGINE/bin/wineserver" -k 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
      return 1
    fi
    sleep 1; waited=$((waited + 1))
  done
  wait "$pid" 2>/dev/null || true
}

# The Battle.net Agent's local HTTP port. Only the agent listens inside the
# bottle at this point; the bottle's wineserver (found by its working directory,
# named after the prefix's device and inode) holds the socket.
agent_port() {
  local dir pid real
  # Wine names the directory after the resolved prefix (see soju-reaper.sh)
  real=$(cd "$WINEPREFIX" && pwd -P)
  dir=$(printf 'server-%x-%x' $(stat -L -f '%d %i' "$real"))
  for pid in $(pgrep -x wineserver); do
    lsof -a -p "$pid" -d cwd -Fn 2>/dev/null | grep -q "/$dir\$" || continue
    lsof -nP -a -p "$pid" -iTCP -sTCP:LISTEN -Fn 2>/dev/null | sed -n 's/^n.*://p' | head -1
    return
  done
}

# Agent 2.41 (Oct 2026) never queues the app install when the setup drives it
# under Wine: the setup sits at 45% forever (issue #60). The agent it unpacked
# installs the app when asked directly through its local API, the same calls
# the setup makes (scripts/bnet-agent-install.js).
install_via_agent() {
  local agents agent port i
  agents="$PROGRAMDATA/Battle.net/Agent"
  agent=$(cd "$agents" 2>/dev/null && ls -d Agent.[0-9]* 2>/dev/null | sort -t. -k2 -n | tail -1)
  [ -n "$agent" ] && [ -f "$agents/$agent/Agent.exe" ] || return 1
  echo "    Finishing the install through the Battle.net Agent ($agent)"
  "$ENGINE/bin/wine" "C:\\ProgramData\\Battle.net\\Agent\\$agent\\Agent.exe" --internalclienttools >/dev/null 2>&1 &
  port=""
  for i in $(seq 1 60); do
    sleep 1; port=$(agent_port)
    [ -z "$port" ] || break
  done
  [ -n "$port" ] || { echo "    The agent did not open its port"; "$ENGINE/bin/wineserver" -k 2>/dev/null || true; return 1; }
  local js="$SCRIPT_DIR/bnet-agent-install.js"
  # Battle.net.exe can land a moment after the agent reports the download done
  if "$ENGINE/bin/wine" cscript //nologo "Z:${js//\//\\}" "$port" "C:\\Program Files (x86)\\Battle.net"; then
    for i in $(seq 1 30); do
      [ ! -f "$BN/Battle.net.exe" ] || break
      sleep 2
    done
  fi
  "$ENGINE/bin/wineserver" -k 2>/dev/null || true
  [ -f "$BN/Battle.net.exe" ]
}

echo "==> Downloading the Battle.net installer (official Blizzard)"
download_setup
set_aside_setup_state

echo "==> Running the installer (a few minutes, fully automatic)"
if ! run_setup; then
  echo "==> The installer did not finish in $(( (SETUP_TIMEOUT + 59) / 60 )) minutes (Battle.net setup can stall at 45%)."
  if ! install_via_agent; then
    echo "    Retrying once from a clean state with a fresh installer download"
    set_aside_setup_state
    rm -f "$SETUP"
    download_setup
    run_setup || install_via_agent || { echo "The installer stalled again, logs: $PROGRAMDATA/Battle.net/Setup"; exit 1; }
  fi
fi

# Wait for the install to finish (up to 10 minutes)
for i in $(seq 1 60); do
  [ -f "$BN/Battle.net.exe" ] && break
  sleep 10
done
[ -f "$BN/Battle.net.exe" ] || { echo "Install failed, logs: $PROGRAMDATA/Battle.net/Setup"; exit 1; }
echo "==> Battle.net installed"

"$ENGINE/bin/wineserver" -k 2>/dev/null || true
echo "==> Done. Now run: scripts/play.sh battlenet"
echo "    (Install games inside the app after logging in to Battle.net)"
