/* soju-gog-restore: bring a running GOG GALAXY window back from the tray.
 *
 * A second GalaxyClient.exe instance does this by finding the main window
 * (class "GalaxyClientClass") and sending it WM_COPYDATA with dwData=1 and a
 * 16-byte block: the message object's vtable pointer inside GalaxyClient.exe
 * (0x140a74858 in 2.1.8.30, dereferenced by the receiver, which works between
 * two copies of the same image) followed by dword 1 (restore). Launching a whole second
 * client for that takes several seconds; this sends the same message directly,
 * which is what the Dock-icon click runs (WINE_DOCK_REOPEN_CMD in play.sh gog).
 *
 * Build: x86_64-w64-mingw32-gcc soju-gog-restore.c -o soju-gog-restore.exe -mwindows -lshell32
 */
#include <windows.h>
#include <shellapi.h>
#include <string.h>

int main(void)
{
    HWND hwnd = FindWindowW(L"GalaxyClientClass", NULL);
    unsigned char block[16];
    COPYDATASTRUCT cds;
    if (!hwnd) return 1;
    memset(block, 0, sizeof(block));
    *(unsigned long long *)block = 0x140a74858ULL;   /* RestoreClientMessage vtable, GalaxyClient 2.1.8.30 */
    block[8] = 1;                       /* command: restore the client window */
    cds.dwData = 1;
    cds.cbData = sizeof(block);
    cds.lpData = block;
    SendMessageTimeoutW(hwnd, WM_COPYDATA, 0, (LPARAM)&cds, SMTO_ABORTIFHUNG, 3000, NULL);
    Sleep(800);
    if (IsWindowVisible(hwnd)) return 0;
    /* The vtable address moved (GOG update): fall back to what GOG itself does,
     * a second client instance, which sends the right message and exits. */
    ShellExecuteW(NULL, L"open", L"C:\\Program Files\\GOG Galaxy\\GalaxyClient.exe", NULL, NULL, SW_SHOWNORMAL);
    return 2;
}
