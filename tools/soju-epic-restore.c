/* soju-epic-restore: bring the Epic Games Launcher back from the tray.
 *
 * Epic hides its Slate window on close and only reopens it from its own tray
 * icon; showing the window from outside (ShowWindow, SC_RESTORE) leaves Slate
 * convinced it is still minimized and the window comes back unresponsive.
 * So this replays exactly what a double-click on the tray icon delivers: the
 * launcher registers its icon with uCallbackMessage 0x8054 (WM_APP+0x54),
 * NOTIFYICON_VERSION 0, so the owner window receives that message with
 * wParam = icon id (1) and lParam = the mouse message. The owner is a hidden
 * window of EpicGamesLauncher.exe, so the sequence is sent to every window of
 * that process; only the tray owner reacts.
 *
 * Build: x86_64-w64-mingw32-gcc -O2 -Wall -mwindows soju-epic-restore.c -o soju-epic-restore.exe -lpsapi
 */
#include <windows.h>
#include <psapi.h>
#include <shellapi.h>
#include <string.h>
#include <wchar.h>

#define EPIC_TRAY_CALLBACK 0x8054
#define EPIC_TRAY_ID       1

static HWND owners[256];
static int nowners;

static BOOL CALLBACK enum_cb(HWND hwnd, LPARAM lp)
{
    WCHAR path[MAX_PATH] = {0};
    DWORD pid = 0;
    HANDLE proc;

    GetWindowThreadProcessId(hwnd, &pid);
    proc = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, FALSE, pid);
    if (!proc) return TRUE;
    GetModuleFileNameExW(proc, NULL, path, MAX_PATH);
    CloseHandle(proc);
    if (wcsstr(path, L"EpicGamesLauncher.exe") && nowners < 256) owners[nowners++] = hwnd;
    return TRUE;
}

static void tray(UINT msg)
{
    int i;
    for (i = 0; i < nowners; i++)
        SendNotifyMessageW(owners[i], EPIC_TRAY_CALLBACK, EPIC_TRAY_ID, msg);
}

int main(void)
{
    EnumWindows(enum_cb, 0);
    if (!nowners) return 1;
    tray(WM_LBUTTONDOWN);   tray(WM_LBUTTONUP);   tray(NIN_SELECT);
    tray(WM_LBUTTONDBLCLK); tray(WM_LBUTTONUP);   tray(NIN_SELECT);
    return 0;
}
