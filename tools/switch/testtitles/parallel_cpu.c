#include "common.h"
#include <stdatomic.h>
#define ITERATIONS 300000000ULL
static Thread workers[4];
static Event start_event;
static _Atomic unsigned ready;
static _Atomic u64 exclusive_count;
static u64 lse_count;
static u64 result[4];
static void worker(void *argument) {
    unsigned id = (uintptr_t)argument;
    atomic_fetch_add_explicit(&ready, 1, memory_order_release);
    eventWait(&start_event, UINT64_MAX);
    u64 count = ITERATIONS, value = id + 1;
    __asm__ volatile("1: add %0, %0, #3\n eor %0, %0, %0, ror #7\n subs %1, %1, #1\n b.ne 1b"
        : "+r"(value), "+r"(count) :: "cc");
    result[id] = value;
    for (unsigned i = 0; i < 10000; ++i) {
        u64 old, next; u32 status;
        __asm__ volatile("1: ldaxr %x[old], [%[address]]\n add %x[next], %x[old], #1\n stlxr %w[status], %x[next], [%[address]]\n cbnz %w[status], 1b"
            : [old] "=&r"(old), [next] "=&r"(next), [status] "=&r"(status)
            : [address] "r"(&exclusive_count) : "memory");
        u64 one = 1, previous;
        __asm__ volatile(".arch_extension lse\n ldaddal %x[in], %x[out], [%[address]]"
            : [out] "=r"(previous) : [in] "r"(one), [address] "r"(&lse_count) : "memory");
    }
}
int main(void) {
    CALL(eventCreate(&start_event, false));
    for (unsigned i = 0; i < 4; ++i) {
        CALL(threadCreate(&workers[i], worker, (void *)(uintptr_t)i, NULL, 0x4000, 44, i));
        CALL(threadStart(&workers[i]));
    }
    while (atomic_load_explicit(&ready, memory_order_acquire) != 4) svcSleepThread(1000000);
    u64 begin = armGetSystemTick();
    eventFire(&start_event);
    for (unsigned i = 0; i < 4; ++i) CALL(threadWaitForExit(&workers[i]));
    u64 end = armGetSystemTick();
    char line[128];
    int n = snprintf(line, sizeof(line), "COMPUTE|ticks=%lu|hz=19200000\n", end - begin);
    svcOutputDebugString(line, n);
    for (unsigned i = 0; i < 4; ++i) {
        u64 expected = i + 1;
        for (u64 j = 0; j < ITERATIONS; ++j) {
            expected += 3;
            expected ^= (expected >> 7) | (expected << 57);
        }
        check("CPU checksum", 0, result[i] == expected);
        CALL(threadClose(&workers[i]));
    }
    check("cross-core LDXR/STXR", 0, exclusive_count == 40000);
    check("cross-core LSE LDADD", 0, lse_count == 40000);
    eventClose(&start_event);
    return finish();
}
