#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
export DEVKITPRO="${DEVKITPRO:-$HOME/devkitpro/opt/devkitpro}"
export DEVKITA64="$DEVKITPRO/devkitA64"
export PATH="$DEVKITPRO/tools/bin:$DEVKITA64/bin:$PATH"
OUT="$ROOT/dist/testtitles"
mkdir -p "$OUT"
for source in "$ROOT"/tools/switch/testtitles/*.c; do
    name="$(basename "$source" .c)"
    aarch64-none-elf-gcc -O2 -g -Wall -Wextra -Werror -march=armv8-a+crc+crypto -mtp=soft -fPIE \
        -D__SWITCH__ -I"$DEVKITPRO/libnx/include" -specs="$DEVKITPRO/libnx/switch.specs" \
        "$source" -L"$DEVKITPRO/libnx/lib" -lnx -o "$OUT/$name.elf"
    nacptool --create "Soju $name probe" "Soju" "1.0.0" "$OUT/$name.nacp"
    elf2nro "$OUT/$name.elf" "$OUT/$name.nro" --nacp="$OUT/$name.nacp" --romfsdir="$ROOT/tools/switch/testtitles/romfs"
    elf2nso "$OUT/$name.elf" "$OUT/$name.nso"
    echo "built $name"
done

build_romfs "$ROOT/tools/switch/testtitles/romfs" "$OUT/probe.romfs"
