#include "abi.h"
static unsigned char heap[1024 * 1024];
static size_t used;
void *malloc(size_t n) {
 n = (n + 15) & ~(size_t)15;
 if (n > sizeof(heap) - used) return 0;
 void *p = heap + used; used += n; return p;
}
void free(void *p) { (void)p; }
void *memcpy(void *d, const void *s, size_t n) {
 unsigned char *out = d; const unsigned char *in = s;
 for (size_t i = 0; i < n; ++i) out[i] = in[i];
 return d;
}
void *memset(void *d, int c, size_t n) {
 unsigned char *out = d; for (size_t i = 0; i < n; ++i) out[i] = c; return d;
}
int memcmp(const void *a, const void *b, size_t n) {
 const unsigned char *x = a, *y = b;
 for (size_t i = 0; i < n; ++i) if (x[i] != y[i]) return x[i] - y[i];
 return 0;
}
size_t strlen(const char *s) { size_t n = 0; while (s[n]) ++n; return n; }
int strcmp(const char *a, const char *b) { while (*a && *a == *b) { ++a; ++b; } return *a - *b; }
void *realloc(void *p, size_t n) { if (p) return 0; return malloc(n); }
void *calloc(size_t n, size_t s) { if (s && n > SIZE_MAX / s) return 0; void *p = malloc(n*s); if (p) memset(p, 0, n*s); return p; }
__attribute__((noreturn)) void abort(void) { probe_check("exception runtime abort", 0); __asm__ volatile("svc 7"); __builtin_unreachable(); }
static void (*exit_callbacks[32])(void);
static unsigned exit_count;
int atexit(void (*f)(void)) { if (exit_count == 32) return -1; exit_callbacks[exit_count++] = f; return 0; }
void probe_run_atexit(void) { while (exit_count) exit_callbacks[--exit_count](); }
char *getenv(const char *name) { (void)name; return 0; }
char *strchr(const char *s, int c) { do { if (*s == c) return (char *)s; } while (*s++); return 0; }
unsigned long strtoul(const char *s, char **end, int base) {
 unsigned long v = 0; if (!base) base = 10;
 while (*s >= '0' && *s <= '9' && *s - '0' < base) v = v * base + *s++ - '0';
 if (end) *end = (char *)s;
 return v;
}
static void *keys[16];
static unsigned key_count;
int pthread_key_create(unsigned *key, void (*destructor)(void *)) {
 if (destructor || key_count == 16) return 22;
 *key = key_count++; return 0;
}
int pthread_key_delete(unsigned key) { if (key >= key_count) return 22; keys[key] = 0; return 0; }
void *pthread_getspecific(unsigned key) { return key < key_count ? keys[key] : 0; }
int pthread_setspecific(unsigned key, const void *value) { if (key >= key_count) return 22; keys[key] = (void *)value; return 0; }
int pthread_once(int *once, void (*initialize)(void)) { if (!*once) { initialize(); *once = 1; } return 0; }
int pthread_mutex_lock(int *lock) { if (*lock) abort(); *lock = 1; return 0; }
int pthread_mutex_unlock(int *lock) { if (!*lock) abort(); *lock = 0; return 0; }
extern void __register_frame(const void *);
void probe_register_frame(const void *frame) { __register_frame(frame); }
extern void _Unwind_Resume(void *) __attribute__((noreturn));
void probe_unwind_resume(void *exception) { _Unwind_Resume(exception); }
