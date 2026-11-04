#include "common.h"
#include <stdatomic.h>
#define WORKERS 64
#define ROUNDS 16
static Thread threads[WORKERS];
static Mutex mutex;
static CondVar ready_condition, start_condition;
static Event event;
static unsigned ready, completed, counter;
static bool start;
static _Atomic u32 address_gate;
static unsigned outcomes[WORKERS];
static __thread unsigned local_value;
static void worker(void *argument) {
    unsigned id = (uintptr_t)argument;
    unsigned errors = svcGetCurrentProcessorNumber() != id % 4;
    local_value = id + 100;
    u64 vector_value = id + 0x123400;
    __asm__ volatile("dup v31.2d, %0\n movz x0, #1, lsl #16\n svc 0xb\n umov %0, v31.d[1]"
        : "+r"(vector_value) :: "x0", "v31", "memory", "cc");
    if (vector_value != id + 0x123400 || local_value != id + 100) ++errors;
    unsigned migrated_core = (id + 1) % 4;
    if (R_FAILED(svcSetThreadCoreMask(CUR_THREAD_HANDLE, migrated_core, 1u << migrated_core))) ++errors;
    if (svcGetCurrentProcessorNumber() != migrated_core) ++errors;
    mutexLock(&mutex);
    ++ready;
    condvarWakeAll(&ready_condition);
    while (!start) {
        if (R_FAILED(condvarWait(&start_condition, &mutex))) ++errors;
    }
    mutexUnlock(&mutex);
    Result rc = svcWaitForAddress((u32 *)&address_gate, ArbitrationType_WaitIfEqual, 0, UINT64_MAX);
    if (R_FAILED(rc) && rc != KERNELRESULT(InvalidState)) ++errors;
    if (R_FAILED(eventWait(&event, UINT64_MAX))) ++errors;
    for (unsigned i = 0; i < ROUNDS; ++i) {
        mutexLock(&mutex);
        unsigned previous = counter;
        svcSleepThread(0); /* Other runnable workers must contend on this lock. */
        counter = previous + 1;
        mutexUnlock(&mutex);
    }
    mutexLock(&mutex);
    ++completed;
    outcomes[id] = errors;
    mutexUnlock(&mutex);
}
int main(void) {
    if (!CALL(eventCreate(&event, false))) return finish();
    for (unsigned i = 0; i < WORKERS; ++i) {
        if (!CALL(threadCreate(&threads[i], worker, (void *)(uintptr_t)i, NULL, 0x4000, 44 + i % 4, i % 4))) return finish();
        s32 priority = 0, core = -1; u64 mask = 0;
        CALL(svcGetThreadPriority(&priority, threads[i].handle));
        CALL(svcGetThreadCoreMask(&core, &mask, threads[i].handle));
        check("priority and affinity", 0, priority == (s32)(44 + i % 4) && core == (s32)(i % 4) && mask == (1u << (i % 4)));
        CALL(threadStart(&threads[i]));
    }
    mutexLock(&mutex);
    while (ready != WORKERS) {
        if (!CALL(condvarWaitTimeout(&ready_condition, &mutex, 5000000000ULL))) return finish();
    }
    start = true;
    CALL(condvarWakeAll(&start_condition));
    mutexUnlock(&mutex);
    atomic_store(&address_gate, 1);
    CALL(svcSignalToAddress((u32 *)&address_gate, SignalType_Signal, 0, -1));
    eventFire(&event);
    for (unsigned i = 0; i < WORKERS; ++i) {
        CALL(threadWaitForExit(&threads[i]));
        check("worker SIMD TLS migration and synchronization", 0, outcomes[i] == 0);
        CALL(threadClose(&threads[i]));
    }
    check("contended counter and all joins", 0, counter == WORKERS * ROUNDS && completed == WORKERS);
    eventClear(&event);
    check("event timeout", 0, eventWait(&event, 1000000) == KERNELRESULT(TimedOut));
    check("address timeout", 0, svcWaitForAddress((u32 *)&address_gate, ArbitrationType_WaitIfEqual, 1, 1000000) == KERNELRESULT(TimedOut));
    mutexLock(&mutex);
    check("condvar timeout reacquires mutex", 0, condvarWaitTimeout(&start_condition, &mutex, 1000000) == KERNELRESULT(TimedOut) && mutexIsLockedByCurrentThread(&mutex));
    mutexUnlock(&mutex);
    eventClose(&event);
    return finish();
}
