#!/bin/sh
# Wine runner for Xbox PC games: `xodus-cli run` calls this with the game's
# decrypted launch view (Z:\...\Game.exe). Same engine and environment as the
# other CX-engine platforms (wine 11.0 from CrossOver sources + D3DMetal),
# on the Xbox prefix that scripts/xbox-prefix.py prepared.
BASE="${SOJU_BASE:-$HOME/.battlenet-macos}"
ENGINE="${ENGINE:-$BASE/cx26-engine}"
export WINEPREFIX="$BASE/xbox-bottle"
export WINEDEBUG="${WINEDEBUG:-fixme-all}"
export WINEMSYNC="${WINEMSYNC:-1}"
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
launch_status=0
"$ENGINE/bin/wine" "$@" || launch_status=$?
# The game may hand off to child processes; the launch ends when the prefix is empty.
wait_status=0
"$ENGINE/bin/wineserver" -w || wait_status=$?
[ "$launch_status" -ne 0 ] && exit "$launch_status"
exit "$wait_status"
