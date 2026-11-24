#!/usr/bin/env bash
# gptk-dlss.sh <engine> <prefix>: make NVIDIA DLSS work in games through
# Apple's MetalFX (D3DMetal's NGX bridge). Run before every launch; it only
# writes when something is missing, and does nothing on an engine without
# Apple's nvngx-on-metalfx shim (no GPTK, or a toolkit older than 3.0).
# Exit status 0 when the bridge is ready (the caller then sets
# D3DM_ENABLE_METALFX=1), 1 otherwise.
#
# Verified 2026-11-24 on an M4 Pro, macOS 26.5, GPTK 3.0: Hogwarts Legacy
# (Epic) lists "NVIDIA DLSS" and the Metal HUD shows MetalFX temporal scaling
# 2295x961 -> 3440x1440. With the engine parts alone, or with them and the
# registry key but Wine's placeholder still in system32, the game offers no DLSS.
#  1. Games and the NGX loader ask for nvngx.dll; Apple ships the shim as
#     nvngx-on-metalfx.dll (CrossOver ships the same file as nvngx.dll), so the
#     engine gets a copy under that name, and the bottle's system32 gets it in
#     place of Wine's placeholder, which the NGX loader does not accept.
#  2. Its unixlib, nvngx.so, must be a symlink to libd3dshared.dylib like the
#     other shims' (get-gptk.sh): a copied file looks for D3DMetal.framework
#     next to itself (@loader_path), fails ("Failed to dlopen D3DMetal") and
#     aborts every process that loads it, the Epic launcher included.
#  3. The NGX loader inside games finds nvngx.dll through
#     HKLM\SOFTWARE\NVIDIA Corporation\Global\NGXCore FullPath, which only
#     NVIDIA's driver writes on Windows.
set -u
ENGINE="${1:?engine}"; PREFIX="${2:?prefix}"
W="$ENGINE/lib/wine/x86_64-windows"; U="$ENGINE/lib/wine/x86_64-unix"
[ -f "$W/nvngx-on-metalfx.dll" ] && [ -f "$ENGINE/lib/external/libd3dshared.dylib" ] || exit 1

if ! cmp -s "$W/nvngx-on-metalfx.dll" "$W/nvngx.dll" 2>/dev/null; then
  cp -f "$W/nvngx-on-metalfx.dll" "$W/nvngx.dll.new" && mv -f "$W/nvngx.dll.new" "$W/nvngx.dll" || exit 1
fi
if [ "$(readlink "$U/nvngx.so" 2>/dev/null)" != ../../external/libd3dshared.dylib ]; then
  rm -f "$U/nvngx.so"; ln -s ../../external/libd3dshared.dylib "$U/nvngx.so" || exit 1
fi

[ -f "$PREFIX/system.reg" ] || exit 1
S="$PREFIX/drive_c/windows/system32"
if [ -d "$S" ] && ! cmp -s "$W/nvngx.dll" "$S/nvngx.dll" 2>/dev/null; then
  cp -f "$W/nvngx.dll" "$S/nvngx.dll.new" && mv -f "$S/nvngx.dll.new" "$S/nvngx.dll" || exit 1
fi
if ! grep -qi '^\[Software\\\\NVIDIA Corporation\\\\Global\\\\NGXCore\]' "$PREFIX/system.reg"; then
  WINEPREFIX="$PREFIX" WINEDEBUG=-all "$ENGINE/bin/wine" reg add 'HKLM\SOFTWARE\NVIDIA Corporation\Global\NGXCore' \
    /v FullPath /t REG_SZ /d 'C:\Windows\System32' /f >/dev/null 2>&1 || exit 1
fi
exit 0
