/* soju-gog-restore: bring a running GOG GALAXY window back from the tray.
 *
 * A second GalaxyClient.exe instance does this by finding the main window
 * (class "GalaxyClientClass") and sending it WM_COPYDATA with dwData=1 and a
 * 16-byte block: the vtable pointer of
 * galaxy::client_base::windows_messaging::RestoreClientMessage inside
 * GalaxyClient.exe (the receiver dereferences it, which works between two
 * copies of the same image) followed by dword 1. Launching a whole second
 * client for that takes several seconds; this sends the same message directly
 * and is what the Dock-icon click runs (WINE_DOCK_REOPEN_CMD in play.sh gog).
 *
 * The vtable moves with every GOG release, so it is located at run time from
 * the MSVC RTTI in the client's exe: type descriptor (by mangled name) ->
 * complete object locator -> vtable, plus the module base of the running
 * client. No version-specific constants.
 *
 * Build: x86_64-w64-mingw32-gcc -O2 -Wall -mwindows soju-gog-restore.c -o soju-gog-restore.exe -lpsapi
 */
#include <windows.h>
#include <psapi.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static const char TYPE_NAME[] = ".?AVRestoreClientMessage@windows_messaging@client_base@galaxy@@";

static DWORD off_to_rva(const BYTE *img, DWORD off)
{
    IMAGE_NT_HEADERS64 *nt = (IMAGE_NT_HEADERS64 *)(img + ((IMAGE_DOS_HEADER *)img)->e_lfanew);
    IMAGE_SECTION_HEADER *s = IMAGE_FIRST_SECTION(nt);
    for (int i = 0; i < nt->FileHeader.NumberOfSections; i++, s++)
        if (off >= s->PointerToRawData && off < s->PointerToRawData + s->SizeOfRawData)
            return s->VirtualAddress + (off - s->PointerToRawData);
    return 0;
}

/* Returns the vtable RVA, 0 when not found. */
static DWORD find_vtable_rva(const BYTE *img, DWORD size)
{
    IMAGE_NT_HEADERS64 *nt = (IMAGE_NT_HEADERS64 *)(img + ((IMAGE_DOS_HEADER *)img)->e_lfanew);
    ULONGLONG pref_base = nt->OptionalHeader.ImageBase;
    DWORD i, td_rva = 0, col_rva = 0;

    /* type descriptor: { vtable ptr; spare ptr; char name[] } */
    for (i = 0; i + sizeof(TYPE_NAME) < size; i++)
        if (!memcmp(img + i, TYPE_NAME, sizeof(TYPE_NAME))) { td_rva = off_to_rva(img, i - 16); break; }
    if (!td_rva) return 0;

    /* complete object locator: { sig=1, offset=0, cdOffset, typeDescriptor RVA, classHierarchy RVA, self RVA } */
    for (i = 0; i + 24 <= size; i += 4)
    {
        const DWORD *d = (const DWORD *)(img + i);
        if (d[0] == 1 && d[1] == 0 && d[3] == td_rva) { col_rva = off_to_rva(img, i); if (d[5] == col_rva) break; }
    }
    if (!col_rva) return 0;

    /* vtable: the qword right before it holds the COL's absolute (preferred) address */
    for (i = 0; i + 16 <= size; i += 8)
        if (*(const ULONGLONG *)(img + i) == pref_base + col_rva) return off_to_rva(img, i + 8);
    return 0;
}

int main(void)
{
    HWND hwnd = FindWindowW(L"GalaxyClientClass", NULL);
    WCHAR path[MAX_PATH];
    DWORD pid = 0, size, read, vt_rva;
    HANDLE proc, file;
    HMODULE mod; DWORD needed;
    BYTE *img;
    unsigned char block[16];
    COPYDATASTRUCT cds;

    if (!hwnd) return 1;
    GetWindowThreadProcessId(hwnd, &pid);
    proc = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, FALSE, pid);
    if (!proc) return 1;
    if (!EnumProcessModules(proc, &mod, sizeof(mod), &needed)) return 1;
    if (!GetModuleFileNameExW(proc, mod, path, MAX_PATH)) return 1;
    CloseHandle(proc);

    file = CreateFileW(path, GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE, NULL, OPEN_EXISTING, 0, NULL);
    if (file == INVALID_HANDLE_VALUE) return 1;
    size = GetFileSize(file, NULL);
    img = malloc(size);
    if (!img || !ReadFile(file, img, size, &read, NULL) || read != size) return 1;
    CloseHandle(file);
    vt_rva = find_vtable_rva(img, size);
    free(img);
    if (!vt_rva) return 3;

    memset(block, 0, sizeof(block));
    *(ULONGLONG *)block = (ULONGLONG)(ULONG_PTR)mod + vt_rva;   /* vtable in the running client */
    block[8] = 1;                                                /* restore */
    cds.dwData = 1;
    cds.cbData = sizeof(block);
    cds.lpData = block;
    SendMessageTimeoutW(hwnd, WM_COPYDATA, 0, (LPARAM)&cds, SMTO_ABORTIFHUNG, 3000, NULL);
    return 0;
}
