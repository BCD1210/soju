#include "common.h"
static Thread thread;
static unsigned errors, visits;
static __thread unsigned local;
static void worker(void *argument) {
    (void)argument;
    local = 37;
    for (unsigned step = 0; step < 16; ++step) {
        unsigned core = step % 4;
        if (R_FAILED(svcSetThreadCoreMask(CUR_THREAD_HANDLE, core, 1u << core))) ++errors;
        if (svcGetCurrentProcessorNumber() != core || local != 37 + step) ++errors;
        ++local;
        ++visits;
    }
}
int main(void) {
    if (!CALL(threadCreate(&thread, worker, NULL, NULL, 0x4000, 44, 0))) return finish();
    if (!CALL(threadStart(&thread))) return finish();
    CALL(threadWaitForExit(&thread));
    check("four-core repeated round trip resumes newest PC and TLS", 0, errors == 0 && visits == 16);
    CALL(threadClose(&thread));
    return finish();
}
