/*
 * skipbp — a minimal just-in-time debugger that swallows breakpoints.
 *
 * Why this exists: Battle.net's bundled libcef hits an int3 (EXCEPTION_BREAKPOINT)
 * on a few threads under Wine. Wine then invokes the AeDebug debugger. The two
 * obvious options both fail:
 *
 *   - No debugger: the unhandled breakpoint kills the process, and Battle.net
 *     restart-loops (or exits outright).
 *   - A debugger that parks the thread forever (the classic
 *     "rundll32 kernel32.dll,Sleep" trick): the process survives, but if the
 *     frozen thread is the renderer's main thread, the CEF login view deadlocks
 *     and never paints — you get an endless spinner.
 *
 * A breakpoint exception leaves EIP/RIP pointing *after* the int3, so a debugger
 * can simply report it handled and let the thread run on. That is all this does:
 * attach, continue every breakpoint with DBG_CONTINUE, pass everything else back
 * to the application, and detach when the process exits.
 *
 * Build (32-bit, matching Battle.net):
 *   i686-w64-mingw32-gcc -O2 -municode -o skipbp.exe skipbp.c
 *
 * Register as the JIT debugger in the prefix (both hives — Battle.net is 32-bit):
 *   HKLM\Software\Microsoft\Windows NT\CurrentVersion\AeDebug
 *   HKLM\Software\Wow6432Node\Microsoft\Windows NT\CurrentVersion\AeDebug
 *     Debugger = C:\windows\skipbp.exe %ld %ld
 *     Auto     = 1
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#include <windows.h>
#include <stdlib.h>

int wmain(int argc, wchar_t **argv)
{
    DWORD pid;
    HANDLE event;
    DEBUG_EVENT ev;

    if (argc < 2) return 1;
    pid = (DWORD)wcstoul(argv[1], NULL, 10);

    if (!DebugActiveProcess(pid)) return 1;
    DebugSetProcessKillOnExit(FALSE);

    /* Wine hands us an event handle to signal once we are attached. */
    if (argc >= 3 && (event = (HANDLE)(ULONG_PTR)wcstoul(argv[2], NULL, 10)) != NULL)
        SetEvent(event);

    for (;;)
    {
        DWORD status = DBG_EXCEPTION_NOT_HANDLED;

        if (!WaitForDebugEvent(&ev, 30000)) break;

        switch (ev.dwDebugEventCode)
        {
        case EXCEPTION_DEBUG_EVENT:
            /* Swallow breakpoints (and single steps) so the thread keeps going;
             * anything else stays the application's problem. */
            if (ev.u.Exception.ExceptionRecord.ExceptionCode == EXCEPTION_BREAKPOINT ||
                ev.u.Exception.ExceptionRecord.ExceptionCode == EXCEPTION_SINGLE_STEP)
                status = DBG_CONTINUE;
            break;

        case EXIT_PROCESS_DEBUG_EVENT:
            ContinueDebugEvent(ev.dwProcessId, ev.dwThreadId, DBG_CONTINUE);
            return 0;

        default:
            status = DBG_CONTINUE;
            break;
        }

        ContinueDebugEvent(ev.dwProcessId, ev.dwThreadId, status);
    }

    DebugActiveProcessStop(pid);
    return 0;
}
