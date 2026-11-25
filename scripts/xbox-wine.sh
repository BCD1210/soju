#!/bin/bash
# Wine runner for Xbox PC games: `xodus-cli run` calls this with the game's
# decrypted launch view (Z:\...\Game.exe). Same engine and environment as the
# other CX-engine platforms (wine 11.0 from CrossOver sources + D3DMetal),
# on the Xbox prefix that scripts/xbox-prefix.py prepared.
BASE="${SOJU_BASE:-$HOME/.battlenet-macos}"
ENGINE="${ENGINE:-$BASE/cx26-engine}"
export WINEPREFIX="$BASE/xbox-bottle"
python3 "$(dirname "${BASH_SOURCE[0]}")/ue_driver_warning.py" "$WINEPREFIX" >/dev/null 2>&1 || true   # Unreal games: no driver warning window
# xbox-run.py drops WINE* variables from the environment; SOJU_XBOX_WINEDEBUG
# passes a Wine debug channel list through for diagnosis.
export WINEDEBUG="${SOJU_XBOX_WINEDEBUG:-${WINEDEBUG:-fixme-all}}"
export WINEMSYNC="${WINEMSYNC:-1}"
[ -f "$BASE/fps-hud" ] && export MTL_HUD_ENABLED=1   # soju fps on
export ROSETTA_ADVERTISE_AVX="${ROSETTA_ADVERTISE_AVX:-1}"
[ "$ROSETTA_ADVERTISE_AVX" = 0 ] && unset ROSETTA_ADVERTISE_AVX
export WINE_SIMULATE_WRITECOPY=1
export CX_ACTIVE_GRAPHICS_BACKEND=d3dmetal
export CX_GRAPHICS_BACKEND=d3dmetal
export CX_APPLEGPTK_LIBD3DSHARED_PATH="$ENGINE/lib/external/libd3dshared.dylib"
export DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib"
if [ "${1:-}" = "--soju-stop-prefix" ]; then
  exec "$ENGINE/bin/wineserver" -k
fi
# NVIDIA DLSS through MetalFX, as in play.sh (scripts/gptk-dlss.sh); after WINEMSYNC is set, since it may start the wineserver.
if [ "${SOJU_DLSS:-1}" = 1 ] && bash "$(dirname "$0")/gptk-dlss.sh" "$ENGINE" "$WINEPREFIX"; then
  export D3DM_ENABLE_METALFX=1
fi
# xodus-cli stops a game by signalling this script. A shell waiting on a
# foreground child ignores SIGINT, so wine runs in the background and the
# signal is forwarded: the game gets a TERM, and anything still running in
# the prefix after a grace period is ended with wineserver -k.
stop() {
  kill -TERM "$wine_pid" 2>/dev/null
  ( sleep 10; "$ENGINE/bin/wineserver" -k ) >/dev/null 2>&1 &
  stopping=1
}
stopping=0
"$ENGINE/bin/wine" "$@" &
wine_pid=$!
trap stop INT TERM HUP
launch_status=0
wait "$wine_pid" || launch_status=$?
# wait returns early when a trapped signal arrives; wait for the real exit.
while kill -0 "$wine_pid" 2>/dev/null; do wait "$wine_pid" || launch_status=$?; done
trap - INT TERM HUP
# The game may hand off to child processes; the launch ends when the prefix is empty.
wait_status=0
"$ENGINE/bin/wineserver" -w || wait_status=$?
[ "$stopping" = 1 ] && exit 130
[ "$launch_status" -ne 0 ] && exit "$launch_status"
exit "$wait_status"
