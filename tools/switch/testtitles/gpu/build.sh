#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
export DEVKITPRO="${DEVKITPRO:-$HOME/devkitpro/opt/devkitpro}"
export DEVKITA64="$DEVKITPRO/devkitA64"
export PATH="$DEVKITPRO/tools/bin:$DEVKITA64/bin:$PATH"
OUT="$ROOT/dist/testtitles"
mkdir -p "$OUT"
aarch64-none-elf-gcc -O2 -g -Wall -Wextra -Werror -march=armv8-a+crc+crypto -mtp=soft -fPIE \
    -D__SWITCH__ -I"$DEVKITPRO/libnx/include" -specs="$DEVKITPRO/libnx/switch.specs" \
    "$ROOT/tools/switch/testtitles/gpu/coherence.c" -L"$DEVKITPRO/libnx/lib" -ldeko3d -lnx -o "$OUT/gpu_coherence.elf"
nacptool --create "Soju GPU coherence probe" "Soju" "1.0.0" "$OUT/gpu_coherence.nacp"
elf2nro "$OUT/gpu_coherence.elf" "$OUT/gpu_coherence.nro" --nacp="$OUT/gpu_coherence.nacp"
