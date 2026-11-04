# Guest dynamic-linker fixture

Run `bash tools/switch/testtitles/dynamic/run.sh`. All generated files stay in
ignored `dist/testtitles/dynamic`. The harness packages four locally built NSOs
in memory using synthetic fixture encryption, then invokes the normal Program
loader and enters the guest rtld. No host relocation or imported-symbol fixup is
performed. Modules are mapped consecutively at page-aligned addresses in the
order rtld, main, subsdk0, sdk. ASLR is not modeled.

The self-written bounded linker reads MOD0 and ELF64 dynamic tags, resolves
symbols through the SysV hash table's symbol count, applies AArch64 RELA records,
and invokes dependency init arrays and reverse fini arrays. Compiler-generated
GLOB_DAT, JUMP_SLOT, RELATIVE, ABS64, DTPMOD64 and DTPREL64 records are executed.
Both imported data mutation and indirect calls are checked in guest code.

The C++ objects are self-written; exception/RTTI support comes from the installed
GCC libsupc++/libgcc, not a platform SDK. Each module's unwind table is registered.
The toolchain hides `_Unwind_Resume`; the build renames that imported reference to
an exported wrapper in sdk. Actual throwing, matching, unwinding, destructor
execution, dynamic_cast and virtual dispatch are exercised.

This is a bounded four-module fixture. Each guest thread lazily receives a DTV
with four independent 256-byte module blocks, initialized from exported template
symbols; `.tbss` bytes remain zero. The fixture owns the pointer at offset 0x200
in its kernel TLS page. Allocation uses an atomic index into 128 lifetime slots.
Eight concurrent workers check imported TLS through 32 migrations each, twice;
closing and recreating threads reuses kernel TLS pages with fresh module blocks.
Main-thread TLS is checked again after both waves. This does not implement a
general-purpose allocator, TLS destructors, unbounded slot recycling or TLSDESC.
The exception support still uses a bounded bump allocator and single-thread
pthread adapters. Exceptions are tested only on main; concurrent exception
handling is not established. Module discovery uses each MOD0 BSS end, not arbitrary sparse mapping
scans. Dependency order is fixed for this acyclic graph. Symbol versions, lazy
binding, dlopen/dlclose, symbol visibility/interposition edge cases, weak undefined
TLS, and arbitrary commercial runtime startup are not established.

Specification references:
- https://switchbrew.org/wiki/NSO0
- https://switchbrew.org/wiki/NRO (MOD0)
- https://github.com/ARM-software/abi-aa/blob/main/aaelf64/aaelf64.rst
- https://refspecs.linuxfoundation.org/elf/gabi4+/ch5.dynamic.html
