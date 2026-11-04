#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
export DEVKITPRO="${DEVKITPRO:-$HOME/devkitpro/opt/devkitpro}"
export PATH="$DEVKITPRO/devkitA64/bin:$DEVKITPRO/tools/bin:$PATH"
SRC="$ROOT/tools/switch/testtitles/dynamic"
OUT="$ROOT/dist/testtitles/dynamic"
mkdir -p "$OUT"
common=(-O2 -g -Wall -Wextra -Werror -fPIC -ffreestanding -fno-stack-protector -mtls-dialect=trad)
for source in sub_objects main_objects; do
 aarch64-none-elf-g++ "${common[@]}" -fno-threadsafe-statics -fno-use-cxa-atexit -c "$SRC/$source.cpp" -o "$OUT/$source.o"
 aarch64-none-elf-objcopy --redefine-sym _Unwind_Resume=probe_unwind_resume "$OUT/$source.o"
done
aarch64-none-elf-gcc "${common[@]}" -c "$SRC/support.c" -o "$OUT/support.o"
for module in rtld main subsdk0 sdk; do
 flags=(-Wl,--build-id=none)
 extra=(-Wl,--hash-style=sysv)
 if [ "$module" = rtld ]; then flags+=(-Wl,-Bsymbolic); fi
 if [ "$module" = main ]; then flags+=(-Dmodule_entry=module_main); extra+=("$OUT/main_objects.o"); fi
 if [ "$module" = subsdk0 ]; then extra+=("$OUT/sub_objects.o"); fi
 if [ "$module" = sdk ]; then extra+=("$OUT/support.o" -Wl,-u,__cxa_throw,-u,__cxa_begin_catch,-u,__cxa_end_catch,-u,__dynamic_cast,-u,__gxx_personality_v0,-u,_ZTVN10__cxxabiv117__class_type_infoE,-u,_ZTVN10__cxxabiv120__si_class_type_infoE,-u,_ZdlPvm,-u,__register_frame,-u,_Unwind_Resume -lsupc++ -lgcc); fi
 aarch64-none-elf-gcc "${common[@]}" -nostdlib -shared -Wl,-T,"$SRC/module.ld" "${flags[@]}" \
  "$SRC/entry.S" "$SRC/$module.c" "${extra[@]}" -o "$OUT/$module.elf"
 elf2nso "$OUT/$module.elf" "$OUT/$module.nso"
 aarch64-none-elf-readelf -rW "$OUT/$module.elf" > "$OUT/$module.relocations.txt"
done
