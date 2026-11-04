/* Bounded fixture linker: ELF64/AArch64 RELA and MOD0, executed entirely in guest. */
#include "abi.h"
#include <elf.h>
typedef struct {
 uintptr_t base;
 Elf64_Dyn *dynamic;
 Elf64_Sym *symbols;
 const char *strings;
 size_t count;
 unsigned char tls[256];
} Module;
static Module modules[4];
static unsigned failures;
static void output(const char *s) {
 size_t n = 0; while (s[n]) ++n;
 register const char *x0 __asm__("x0") = s;
 register size_t x1 __asm__("x1") = n;
 __asm__ volatile("svc 0x27" : "+r"(x0) : "r"(x1) : "memory");
}
static __attribute__((noreturn)) void stop(void) {
 __asm__ volatile("svc 7" ::: "memory");
 __builtin_unreachable();
}
static __attribute__((noreturn)) void fatal(const char *s) {
 output("FAIL|"); output(s); output("\n"); stop();
}
void probe_check(const char *s, int ok) {
 if (!ok) ++failures;
 output(ok ? "PASS|" : "FAIL|"); output(s); output("\n");
}
static int equal(const char *a, const char *b) {
 while (*a && *a == *b) { ++a; ++b; } return *a == *b;
}
static uintptr_t tag(Module *m, long wanted) {
 for (Elf64_Dyn *d = m->dynamic; d->d_tag; ++d)
  if (d->d_tag == wanted) return d->d_un.d_val;
 return 0;
}
static Elf64_Sym *find(Module *m, const char *name) {
 for (size_t i = 1; i < m->count; ++i)
  if (m->symbols[i].st_shndx && equal(m->strings + m->symbols[i].st_name, name)) return &m->symbols[i];
 return 0;
}
static uintptr_t resolve(const char *name) {
 for (unsigned i = 0; i < 4; ++i) {
  Elf64_Sym *s = find(&modules[i], name);
  if (s) return modules[i].base + s->st_value;
 }
 fatal(name);
}
void *__tls_get_addr(const uintptr_t *index) {
 if (index[0] >= 4 || index[1] >= 256) fatal("TLS index");
 return modules[index[0]].tls + index[1];
}
static void relocate(Module *m, Elf64_Rela *r, size_t bytes) {
 for (size_t i = 0; i < bytes / sizeof(*r); ++i) {
  unsigned type = ELF64_R_TYPE(r[i].r_info), sym = ELF64_R_SYM(r[i].r_info);
  uintptr_t *target = (uintptr_t *)(m->base + r[i].r_offset);
  if (type == R_AARCH64_RELATIVE) { *target = m->base + r[i].r_addend; continue; }
  if (type == R_AARCH64_NONE) continue;
  Module *owner = 0; Elf64_Sym *definition = 0;
  if (!sym) { owner = m; }
  else for (unsigned j = 0; j < 4; ++j) {
   definition = find(&modules[j], m->strings + m->symbols[sym].st_name);
   if (definition) { owner = &modules[j]; break; }
  }
  if (!owner) {
   if (ELF64_ST_BIND(m->symbols[sym].st_info) == STB_WEAK) { *target = 0; continue; }
   fatal(m->strings + m->symbols[sym].st_name);
  }
  switch (type) {
   case R_AARCH64_GLOB_DAT: case R_AARCH64_JUMP_SLOT: case R_AARCH64_ABS64:
    *target = owner->base + definition->st_value + r[i].r_addend; break;
   case R_AARCH64_TLS_DTPMOD: *target = (uintptr_t)(owner - modules); break;
   case R_AARCH64_TLS_DTPREL: *target = (definition ? definition->st_value : 0) + r[i].r_addend; break;
   default: fatal("unsupported relocation");
  }
 }
}
static void arrays(Module *m, long pointer, long size) {
 uintptr_t offset = tag(m, pointer);
 if (!offset) return;
 void (**functions)(void) = (void (**)(void))(m->base + offset);
 for (size_t i = 0; i < tag(m, size) / sizeof(*functions); ++i) functions[i]();
}
void module_entry(uintptr_t base) {
 /* Bootstrap only relative relocations before accessing the linker's own GOT. */
 int32_t *mod = (int32_t *)(base + *(uint32_t *)(base + 4));
 Module self = { .base = base, .dynamic = (Elf64_Dyn *)((uintptr_t)mod + mod[1]) };
 Elf64_Rela *r = (Elf64_Rela *)(base + tag(&self, DT_RELA));
 for (size_t i = 0; i < tag(&self, DT_RELASZ) / sizeof(*r); ++i)
  if (ELF64_R_TYPE(r[i].r_info) == R_AARCH64_RELATIVE)
   *(uintptr_t *)(base + r[i].r_offset) = base + r[i].r_addend;
 for (unsigned i = 0; i < 4; ++i) {
  Module *m = &modules[i]; m->base = base;
  mod = (int32_t *)(base + *(uint32_t *)(base + 4));
  if (mod[0] != 0x30444f4d) fatal("MOD0 magic/order");
  m->dynamic = (Elf64_Dyn *)((uintptr_t)mod + mod[1]);
  m->symbols = (Elf64_Sym *)(base + tag(m, DT_SYMTAB));
  m->strings = (const char *)(base + tag(m, DT_STRTAB));
  m->count = ((uint32_t *)(base + tag(m, DT_HASH)))[1];
  base = ((uintptr_t)mod + mod[3] + 4095) & ~(uintptr_t)4095;
 }
 for (unsigned i = 0; i < 4; ++i) {
  Module *m = &modules[i];
  relocate(m, (Elf64_Rela *)(m->base + tag(m, DT_RELA)), tag(m, DT_RELASZ));
  relocate(m, (Elf64_Rela *)(m->base + tag(m, DT_JMPREL)), tag(m, DT_PLTRELSZ));
  Elf64_Sym *start = find(m, "__probe_tls_start"), *size = find(m, "__probe_tls_size");
  if (start && size) {
   if (size->st_value > sizeof(m->tls)) fatal("TLS size");
   for (size_t j = 0; j < size->st_value; ++j) m->tls[j] = *(unsigned char *)(m->base + start->st_value + j);
  }
 }
 void (*register_frame)(const void *) = (void (*)(const void *))resolve("probe_register_frame");
 for (unsigned i = 1; i < 4; ++i) {
  Elf64_Sym *frame = find(&modules[i], "__probe_eh_frame");
  if (frame) register_frame((const void *)(modules[i].base + frame->st_value));
 }
 for (unsigned i = 4; i-- > 1;) arrays(&modules[i], DT_INIT_ARRAY, DT_INIT_ARRAYSZ);
 ((void (*)(void))resolve("module_main"))();
 ((void (*)(void))resolve("probe_run_atexit"))();
 for (unsigned i = 1; i < 4; ++i) arrays(&modules[i], DT_FINI_ARRAY, DT_FINI_ARRAYSZ);
 probe_check("reverse fini arrays", *(int *)resolve("lifecycle") == 123321);
 if (!failures) output("TOTAL|dynamic-modules|failures=0\n");
 stop();
}
