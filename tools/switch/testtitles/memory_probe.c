#include "common.h"
#include <stdlib.h>
u32 __nx_heap_size = 32 * 1024 * 1024;
static bool query(const char *name, void *address, u32 type, u32 perm) {
    MemoryInfo info; u32 page;
    Result rc = svcQueryMemory(&info, &page, (u64)address);
    return check(name, rc, (info.type & 0xff) == type && info.perm == perm);
}
int main(void) {
    void *heap = NULL;
    if (!CALL(svcSetHeapSize(&heap, 32 * 1024 * 1024))) return finish();
    u32 *tail = (u32 *)((char *)heap + 32 * 1024 * 1024);
    if (!CALL(svcSetHeapSize(&heap, 34 * 1024 * 1024))) return finish();
    check("heap growth zero fill", 0, tail[0] == 0);
    tail[0] = 0x12345678;
    CALL(svcSetHeapSize(&heap, 32 * 1024 * 1024));
    query("heap shrink releases tail", tail, MemType_Unmapped, 0);
    CALL(svcSetHeapSize(&heap, 34 * 1024 * 1024));
    check("regrowth clears reused backing", 0, tail[0] == 0);
    u64 alias;
    if (!CALL(svcGetInfo(&alias, InfoType_AliasRegionAddress, CUR_PROCESS_HANDLE, 0))) return finish();
    void *physical = (void *)alias;
    check("physical zero size rejected", 0, R_FAILED(svcMapPhysicalMemory(physical, 0)));
    check("physical outside alias region rejected", 0, R_FAILED(svcMapPhysicalMemory(heap, 0x1000)));
    if (!CALL(svcMapPhysicalMemory(physical, 0x2000))) return finish();
    ((u32 *)physical)[0] = 23; ((u32 *)physical)[1024] = 45;
    check("physical pages read/write", 0, ((u32 *)physical)[0] + ((u32 *)physical)[1024] == 68);
    check("physical overlap rejected", 0, R_FAILED(svcMapPhysicalMemory(physical, 0x2000)));
    CALL(svcUnmapPhysicalMemory(physical, 0x2000));
    query("physical unmap", physical, MemType_Unmapped, 0);
    CALL(svcMapPhysicalMemory(physical, 0x2000));
    check("physical reallocation clears pages", 0, ((u32 *)physical)[0] == 0 && ((u32 *)physical)[1024] == 0);
    CALL(svcUnmapPhysicalMemory(physical, 0x2000));
    Handle transfer;
    if (!CALL(svcCreateTransferMemory(&transfer, tail, 0x1000, Perm_None))) return finish();
    query("transfer locks source", tail, MemType_Heap, 0);
    check("locked transfer blocks shrink", 0, R_FAILED(svcSetHeapSize(&heap, 32 * 1024 * 1024)));
    CALL(svcCloseHandle(transfer));
    tail[0] = 77;
    check("transfer close restores source", 0, tail[0] == 77);
    Handle process = envGetOwnProcessHandle();
    check("real process handle supplied", 0, process != INVALID_HANDLE && process != CUR_PROCESS_HANDLE);
    u64 code = 0x600000000;
    tail[0] = 0x52800540; tail[1] = 0xd65f03c0; /* mov w0,#42; ret */
    check("pseudo process alias rejected", 0, R_FAILED(svcMapProcessCodeMemory(CUR_PROCESS_HANDLE, code, (u64)tail, 0x1000)));
    if (!CALL(svcMapProcessCodeMemory(process, code, (u64)tail, 0x1000))) return finish();
    check("code overlap rejected", 0, R_FAILED(svcMapProcessCodeMemory(process, code, (u64)tail, 0x1000)));
    check("mismatched code unmap rejected", 0, R_FAILED(svcUnmapProcessCodeMemory(process, code, (u64)tail, 0x2000)));
    query("code mapping locks heap source", tail, MemType_Heap, 0);
    check("code alias blocks shrink", 0, R_FAILED(svcSetHeapSize(&heap, 32 * 1024 * 1024)));
    check("RWX rejected", 0, R_FAILED(svcSetProcessMemoryPermission(process, code, 0x1000, 7)));
    if (!CALL(svcSetProcessMemoryPermission(process, code, 0x1000, Perm_Rx))) return finish();
    check("execute mapped process code", 0, ((int (*)(void))code)() == 42);
    CALL(svcUnmapProcessCodeMemory(process, code, (u64)tail, 0x1000));
    query("code unmap", (void *)code, MemType_Unmapped, 0);
    check("code source restored", 0, tail[0] == 0x52800540);
    tail[0] = 0x52800560; /* mov w0,#43 */
    CALL(svcMapProcessCodeMemory(process, code, (u64)tail, 0x1000));
    CALL(svcSetProcessMemoryPermission(process, code, 0x1000, Perm_Rx));
    check("remapped code observes new instructions", 0, ((int (*)(void))code)() == 43);
    CALL(svcUnmapProcessCodeMemory(process, code, (u64)tail, 0x1000));
    if (!CALL(svcSetHeapSize(&heap, 3ULL * 1024 * 1024 * 1024))) return finish();
    volatile u64 *low = (void *)((char *)heap + (64ULL << 20)), *middle = (void *)((char *)heap + (1ULL << 31));
    volatile u64 *high = (void *)((char *)heap + (3ULL << 30) - 8);
    *low = 11; *middle = 22; *high = 33;
    check("3 GiB heap distinct low middle high pages", 0, *low == 11 && *middle == 22 && *high == 33);
    CALL(svcSetHeapSize(&heap, 32 * 1024 * 1024));
    query("large heap shrink", (void *)high, MemType_Unmapped, 0);
    return finish();
}
