#include "common.h"
#include <stdlib.h>
#include <stdatomic.h>
static Thread reader;
static _Atomic unsigned published, consumed;
static unsigned errors;
static const uintptr_t owner = 0x600000000, slave = 0x600010000;
static void execute(void *unused) {
    (void)unused;
    for (unsigned round = 1; round <= 32; ++round) {
        while (atomic_load_explicit(&published, memory_order_acquire) != round) svcSleepThread(0);
        if (((unsigned (*)(void))slave)() != round) ++errors;
        atomic_store_explicit(&consumed, round, memory_order_release);
    }
}
int main(void) {
    void *backing = aligned_alloc(4096, 4096);
    if (!check("code backing allocated", 0, backing != NULL)) return finish();
    Handle handle;
    check("unaligned source rejected", 0, R_FAILED(svcCreateCodeMemory(&handle, (char *)backing + 1, 4096)));
    if (!CALL(svcCreateCodeMemory(&handle, backing, 4096))) return finish();
    check("owner RX rejected", 0, R_FAILED(svcControlCodeMemory(handle, CodeMapOperation_MapOwner, (void *)owner, 4096, Perm_Rx)));
    if (!CALL(svcControlCodeMemory(handle, CodeMapOperation_MapOwner, (void *)owner, 4096, Perm_Rw))) return finish();
    check("code backing starts FF", 0, *(u32 *)owner == UINT32_MAX);
    if (!CALL(svcControlCodeMemory(handle, CodeMapOperation_MapSlave, (void *)slave, 4096, Perm_Rx))) return finish();
    MemoryInfo info; u32 page;
    CALL(svcQueryMemory(&info, &page, slave));
    check("slave RX and generated-code type", 0, info.perm == Perm_Rx && (info.type & 0xff) == MemType_CodeReadOnly);
    CALL(threadCreate(&reader, execute, NULL, NULL, 0x4000, 44, 2));
    CALL(threadStart(&reader));
    for (unsigned round = 1; round <= 32; ++round) {
        ((u32 *)owner)[0] = 0x52800000 | (round << 5); /* mov w0,#round */
        ((u32 *)owner)[1] = 0xd65f03c0; /* ret */
        armDCacheFlush((void *)owner, 64);
        armICacheInvalidate((void *)slave, 64);
        atomic_store_explicit(&published, round, memory_order_release);
        while (atomic_load_explicit(&consumed, memory_order_acquire) != round) svcSleepThread(0);
    }
    CALL(threadWaitForExit(&reader));
    CALL(threadClose(&reader));
    check("cross-core executable alias publication 32 rounds", 0, errors == 0);
    check("wrong unmap size rejected", 0, R_FAILED(svcControlCodeMemory(handle, CodeMapOperation_UnmapSlave, (void *)slave, 8192, Perm_None)));
    CALL(svcControlCodeMemory(handle, CodeMapOperation_UnmapSlave, (void *)slave, 4096, Perm_None));
    CALL(svcControlCodeMemory(handle, CodeMapOperation_UnmapOwner, (void *)owner, 4096, Perm_None));
    CALL(svcCloseHandle(handle));
    check("closing object restores backing", 0, *(u32 *)backing == (0x52800000 | (32 << 5)));
    free(backing);
    return finish();
}
