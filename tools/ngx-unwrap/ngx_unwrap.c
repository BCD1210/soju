/* nvngx.dll for the D2R VR copy (scripts/d2r-vr.py).
 *
 * The VR mod runs the game under ReShade, which hands the game its own wrappers of the
 * D3D12 device and command lists. Apple's NGX shim (D3DMetal's DLSS -> MetalFX bridge)
 * treats the command list it is given as D3DMetal's own object, so a ReShade wrapper
 * crashes NVSDK_NGX_D3D12_CreateFeature and the game hangs in its crash handler at the
 * title (seen 2026-11-25, D2R 3.4, ReShade 6.8.0, GPTK 3.0).
 *
 * This DLL sits in front of the shim (installed as nvngx_metalfx.dll) and passes it the
 * original objects. A ReShade 6 proxy starts with its COM vtable, then the
 * reshade::api object vtable, both inside ReShade64.dll, then the original pointer.
 * Anything that does not look like that is passed through unchanged. Every other
 * export is forwarded as is. */
#include <windows.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

static HMODULE real, reshade;

static HMODULE owner(const void *p)
{
    HMODULE m = NULL;
    if (!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, p, &m))
        return NULL;
    return m;
}

static BOOL readable(const void *p, SIZE_T n)
{
    MEMORY_BASIC_INFORMATION mbi;
    if (!p || !VirtualQuery(p, &mbi, sizeof(mbi)) || mbi.State != MEM_COMMIT) return FALSE;
    if (mbi.Protect & (PAGE_NOACCESS | PAGE_GUARD)) return FALSE;
    return (const char *)p + n <= (const char *)mbi.BaseAddress + mbi.RegionSize;
}

static void *unwrap(void *obj)
{
    void *orig;
    if (!reshade) reshade = GetModuleHandleW(L"ReShade64.dll");
    if (!reshade || !readable(obj, 3 * sizeof(void *))) return obj;
    if (owner(*(void **)obj) != reshade) return obj;          /* not a ReShade proxy */
    if (owner(((void **)obj)[1]) != reshade) return obj;      /* not the layout above */
    orig = ((void **)obj)[2];
    /* D3DMetal's objects live in host memory outside the Windows address map, so
     * VirtualQuery cannot vouch for them: only reject what clearly is not one. */
    if ((uintptr_t)orig < 0x10000 || owner(orig) == reshade) return obj;
    if (readable(orig, sizeof(void *)) && owner(*(void **)orig) == reshade) return obj;
    return orig;
}

static void note(const char *what, void *in, void *out)
{
    FILE *f;
    if (!getenv("SOJU_NGX_LOG") || !(f = fopen("C:\\soju-ngx.log", "a"))) return;
    fprintf(f, "%s %p -> %p%s\n", what, in, out, in == out ? " (passed through)" : "");
    fclose(f);
}

static FARPROC get(const char *name)
{
    if (!real) real = LoadLibraryW(L"nvngx_metalfx.dll");
    return real ? GetProcAddress(real, name) : NULL;
}

#define NGX_FAIL 0xBAD00000u   /* NVSDK_NGX_Result_Fail */
typedef unsigned int (*f4)(void *, uintptr_t, uintptr_t, uintptr_t);
typedef unsigned int (*f5)(uintptr_t, uintptr_t, void *, uintptr_t, uintptr_t);
typedef unsigned int (*f7)(uintptr_t, uintptr_t, uintptr_t, uintptr_t, void *, uintptr_t, uintptr_t);
#define REAL(name) static FARPROC p; void *u; if (!p && !(p = get(#name))) return NGX_FAIL

__declspec(dllexport) unsigned int NVSDK_NGX_D3D12_Init(uintptr_t a, uintptr_t b, void *dev, uintptr_t d, uintptr_t e)
{
    REAL(NVSDK_NGX_D3D12_Init);
    u = unwrap(dev); note(__func__, dev, u);
    return ((f5)p)(a, b, u, d, e);
}
__declspec(dllexport) unsigned int NVSDK_NGX_D3D12_Init_Ext(uintptr_t a, uintptr_t b, void *dev, uintptr_t d, uintptr_t e)
{
    REAL(NVSDK_NGX_D3D12_Init_Ext);
    u = unwrap(dev); note(__func__, dev, u);
    return ((f5)p)(a, b, u, d, e);
}
__declspec(dllexport) unsigned int NVSDK_NGX_D3D12_Init_ProjectID(uintptr_t a, uintptr_t b, uintptr_t c, uintptr_t d, void *dev, uintptr_t f, uintptr_t g)
{
    REAL(NVSDK_NGX_D3D12_Init_ProjectID);
    u = unwrap(dev); note(__func__, dev, u);
    return ((f7)p)(a, b, c, d, u, f, g);
}
__declspec(dllexport) unsigned int NVSDK_NGX_D3D12_CreateFeature(void *list, uintptr_t b, uintptr_t c, uintptr_t d)
{
    REAL(NVSDK_NGX_D3D12_CreateFeature);
    u = unwrap(list); note(__func__, list, u);
    return ((f4)p)(u, b, c, d);
}
__declspec(dllexport) unsigned int NVSDK_NGX_D3D12_EvaluateFeature(void *list, uintptr_t b, uintptr_t c, uintptr_t d)
{
    REAL(NVSDK_NGX_D3D12_EvaluateFeature);
    u = unwrap(list);
    return ((f4)p)(u, b, c, d);
}

BOOL WINAPI DllMain(HINSTANCE h, DWORD reason, void *reserved)
{
    if (reason == DLL_PROCESS_ATTACH) DisableThreadLibraryCalls(h);
    return TRUE;
}
