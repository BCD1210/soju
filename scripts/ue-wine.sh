#!/bin/bash
# soju ue <folder> [game arguments]: start a packaged Unreal Engine 4 or 5
# Windows build that you have as files (your own project's package, or a build
# a developer sent you) on the CX engine with D3DMetal, in a bottle of its own
# per project (ue-bottles/<Project>). Same engine and environment as
# scripts/xbox-wine.sh.
#
# <folder> is the package (<Project>.exe next to <Project>/ and Engine/). That
# exe only starts <Project>/Binaries/Win64/<Project>-Win64-Shipping.exe, which
# is started directly here; the exe at the top is used when there is no
# Shipping exe. Without game arguments the game opens windowed at 1280x720.
set -u
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BASE="${SOJU_BASE:-$HOME/.battlenet-macos}"
ENGINE="${ENGINE:-$BASE/cx26-engine}"
FOLDER="${1:?usage: soju ue <folder with the packaged Windows build> [game arguments]}"; shift
[ $# -gt 0 ] || set -- -ResX=1280 -ResY=720 -windowed

shopt -s nullglob
found=("$FOLDER"/*/Binaries/Win64/*-Win64-Shipping.exe "$FOLDER"/Binaries/Win64/*-Win64-Shipping.exe)
[ ${#found[@]} -gt 0 ] || found=("$FOLDER"/*.exe)
shopt -u nullglob
[ ${#found[@]} -gt 0 ] || { echo "soju: no <Project>-Win64-Shipping.exe or <Project>.exe in $FOLDER" >&2; exit 1; }
EXE="${found[0]}"
NAME="$(basename "$EXE" .exe)"; NAME="${NAME%-Win64-Shipping}"

[ -x "$ENGINE/bin/wine" ] || { echo "soju: the engine is not installed; run: soju install" >&2; exit 1; }
[ -f "$ENGINE/lib/external/libd3dshared.dylib" ] || { echo "soju: Unreal's Direct3D 12 needs Apple's Game Porting Toolkit; run: soju gptk" >&2; exit 1; }
W="$ENGINE/bin/wine"; WS="$ENGINE/bin/wineserver"
export WINEPREFIX="$BASE/ue-bottles/$NAME"
export WINEDEBUG="${WINEDEBUG:-fixme-all}"
export WINEMSYNC="${WINEMSYNC:-1}"
[ -f "$BASE/fps-hud" ] && export MTL_HUD_ENABLED=1   # soju fps on
export ROSETTA_ADVERTISE_AVX="${ROSETTA_ADVERTISE_AVX:-1}"
[ "$ROSETTA_ADVERTISE_AVX" = 0 ] && unset ROSETTA_ADVERTISE_AVX
export WINE_SIMULATE_WRITECOPY=1
export CX_ACTIVE_GRAPHICS_BACKEND=d3dmetal
export CX_GRAPHICS_BACKEND=d3dmetal
export CX_APPLEGPTK_LIBD3DSHARED_PATH="$ENGINE/lib/external/libd3dshared.dylib"
export DYLD_FALLBACK_LIBRARY_PATH="$ENGINE/lib:/usr/lib"

mkdir -p "$WINEPREFIX" "$BASE/logs"
LOG="$BASE/logs/ue-$NAME.log"
: > "$LOG"
if [ ! -f "$WINEPREFIX/system.reg" ]; then
  echo "Creating the bottle $WINEPREFIX"
  "$W" wineboot -u >>"$LOG" 2>&1
  "$WS" -w
fi

# Unreal builds link Microsoft's VC++ 2015-2022 x64 runtime. It is the installer
# the Xbox prefix uses (scripts/xbox-prefix.py), downloaded from Microsoft and
# checked by SHA-256, never redistributed.
if [ ! -f "$WINEPREFIX/.soju-vcrun" ]; then
  read -r vc_version vc_sha vc_url < <(python3 -c 'import runpy, sys
v = runpy.run_path(sys.argv[1])["VC"]; print(v["version"], v["sha256"], v["url"])' "$DIR/xbox-prefix.py")
  vc="$BASE/build/vc_redist.x64-$vc_version.exe"
  if [ "$(shasum -a 256 "$vc" 2>/dev/null | cut -d' ' -f1)" != "$vc_sha" ]; then
    echo "Downloading the Microsoft VC++ runtime"
    mkdir -p "$BASE/build"
    curl --fail --silent --show-error --location --proto =https --proto-redir =https --max-time 600 \
      "$vc_url" --output "$vc.part" && mv -f "$vc.part" "$vc"
    if [ "$(shasum -a 256 "$vc" 2>/dev/null | cut -d' ' -f1)" != "$vc_sha" ]; then
      rm -f "$vc" "$vc.part"
      echo "soju: the VC++ runtime download failed or did not match its checksum" >&2; exit 1
    fi
  fi
  echo "Installing the VC++ runtime into the bottle"
  "$W" "$vc" /install /quiet /norestart >>"$LOG" 2>&1; code=$?
  "$WS" -w
  # 1638 (a same or newer version is installed) and 3010 (restart pending) as an 8-bit exit status.
  case "$code" in
    0|102|194) echo "$vc_version" > "$WINEPREFIX/.soju-vcrun" ;;
    *) echo "soju: the VC++ runtime did not install (code $code); Wine log: $LOG" >&2; exit 1 ;;
  esac
fi

python3 "$DIR/ue_driver_warning.py" "$WINEPREFIX" >/dev/null 2>&1 || true   # no driver warning window

echo "Starting $EXE"
echo "The first start converts the game's shaders for Metal and can take several minutes. Wine log: $LOG"
cd "$(dirname "$EXE")" || exit 1
# The bottle holds only this game: Ctrl+C or closing the Terminal ends all of it.
trap '"$WS" -k; exit 130' INT TERM HUP
"$W" "$(basename "$EXE")" "$@" >>"$LOG" 2>&1 &
wait $!
status=$?
"$WS" -w
[ "$status" = 0 ] || echo "soju: the game exited with code $status; Wine log: $LOG" >&2
exit "$status"
