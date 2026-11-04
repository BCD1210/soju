/* Original bounded thread fixture, using the public SVC register ABI. */
#define TLS_WORKERS 8
static unsigned char stacks[TLS_WORKERS][16384] __attribute__((aligned(16)));
static unsigned results[TLS_WORKERS];
static uintptr_t addresses[TLS_WORKERS];
extern __thread int sdk_local, sdk_zero;
static void tls_worker(uintptr_t id) {
 unsigned errors = sdk_zero != 0;
 sdk_zero = (int)id + 71;
 addresses[id] = (uintptr_t)&sdk_local;
 for (unsigned round = 0; round < 32; ++round) {
  if (sdk_zero != (int)id + 71) ++errors;
  if (sdk_tls() != (int)(6 + round) || sub_tls() != (int)(101 + round)) ++errors;
  register uintptr_t x0 __asm__("x0") = 0xffff8000;
  register uintptr_t x1 __asm__("x1") = (id + round) % 4;
  register uintptr_t x2 __asm__("x2") = 1u << x1;
  __asm__ volatile("svc 0xf" : "+r"(x0) : "r"(x1), "r"(x2) : "memory");
  if (x0) ++errors;
  x0 = 0;
  __asm__ volatile("svc 0xb" : "+r"(x0) :: "memory");
 }
 results[id] = errors;
 __asm__ volatile("svc 0xa" ::: "memory");
 __builtin_unreachable();
}
static void check_thread_tls(void) {
 for (unsigned wave = 0; wave < 2; ++wave) {
  uint32_t handles[TLS_WORKERS];
  unsigned errors = 0;
  for (unsigned i = 0; i < TLS_WORKERS; ++i) {
   register uintptr_t x0 __asm__("x0") = 0;
   register uintptr_t x1 __asm__("x1") = (uintptr_t)tls_worker;
   register uintptr_t x2 __asm__("x2") = i;
   register uintptr_t x3 __asm__("x3") = (uintptr_t)&stacks[i][sizeof(stacks[i])];
   register uintptr_t x4 __asm__("x4") = 44;
   register uintptr_t x5 __asm__("x5") = i % 4;
   __asm__ volatile("svc 8" : "+r"(x0), "+r"(x1) : "r"(x2), "r"(x3), "r"(x4), "r"(x5) : "memory");
   if (x0) { probe_check("create TLS worker", 0); return; }
   handles[i] = x1;
   x0 = handles[i];
   __asm__ volatile("svc 9" : "+r"(x0) :: "memory");
   if (x0) { probe_check("start TLS worker", 0); return; }
  }
  for (unsigned i = 0; i < TLS_WORKERS; ++i) {
   register uintptr_t x0 __asm__("x0") = 0;
   register uintptr_t x1 __asm__("x1") = (uintptr_t)&handles[i];
   register uintptr_t x2 __asm__("x2") = 1;
   register uintptr_t x3 __asm__("x3") = UINT64_MAX;
   __asm__ volatile("svc 0x18" : "+r"(x0), "+r"(x1) : "r"(x2), "r"(x3) : "memory");
   if (x0 || results[i]) ++errors;
   x0 = handles[i];
   __asm__ volatile("svc 0x16" : "+r"(x0) :: "memory");
   if (x0) ++errors;
   for (unsigned j = 0; j < i; ++j) if (addresses[i] == addresses[j]) ++errors;
   if (addresses[i] == (uintptr_t)&sdk_local) ++errors;
  }
  probe_check(wave ? "reused kernel TLS pages receive fresh module blocks" : "eight threads retain independent module TLS through migration", errors == 0);
 }
}
