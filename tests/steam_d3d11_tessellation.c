#define COBJMACROS
#include <windows.h>
#include <d3d11.h>
#include <d3dcompiler.h>
#include <stdio.h>
#include <string.h>

/* Tessellation load check for DXMT's d3d11.maxTessFactor option. Draws many
   patches whose hull shader asks for factor 64 and prints the average GPU
   frame time. Run it once without and once with a dxmt.conf in the working
   directory; a capped factor should cut the frame time. The optional argument
   sets how many patches are drawn per frame (default 1600). */
static const char hlsl[] =
    "struct VSOut { float4 pos : POSITION; };\n"
    "VSOut vs(uint id : SV_VertexID) {\n"
    "  uint patch = id / 3, corner = id % 3;\n"
    "  float x = (patch % 40) / 20.0 - 1.0, y = ((patch / 40) % 40) / 20.0 - 1.0;\n"
    "  float2 p = float2(x, y) + float2(corner == 1 ? 0.05 : 0.0, corner == 2 ? 0.05 : 0.0);\n"
    "  VSOut o; o.pos = float4(p, 0.5, 1); return o; }\n"
    "struct PC { float e[3] : SV_TessFactor; float i : SV_InsideTessFactor; };\n"
    "PC pc(InputPatch<VSOut, 3> ip) { PC o; o.e[0] = o.e[1] = o.e[2] = 64; o.i = 64; return o; }\n"
    "[domain(\"tri\")] [partitioning(\"integer\")] [outputtopology(\"triangle_cw\")]\n"
    "[outputcontrolpoints(3)] [patchconstantfunc(\"pc\")] [maxtessfactor(64.0)]\n"
    "VSOut hs(InputPatch<VSOut, 3> ip, uint i : SV_OutputControlPointID) { return ip[i]; }\n"
    "[domain(\"tri\")]\n"
    "float4 ds(PC c, float3 b : SV_DomainLocation, const OutputPatch<VSOut, 3> p) : SV_Position {\n"
    "  float4 pos = p[0].pos * b.x + p[1].pos * b.y + p[2].pos * b.z;\n"
    "  pos.y += 0.002 * sin(b.x * 40.0); return pos; }\n"
    "float4 ps() : SV_Target { return float4(0.9, 0.6, 0.2, 1); }\n";

typedef HRESULT (WINAPI *compile_fn)(const void *, SIZE_T, const char *, const D3D_SHADER_MACRO *,
    ID3DInclude *, const char *, const char *, UINT, UINT, ID3DBlob **, ID3DBlob **);

static ID3DBlob *compile(compile_fn fn, const char *entry, const char *target) {
    ID3DBlob *code = NULL, *errors = NULL;
    HRESULT hr = fn(hlsl, sizeof(hlsl) - 1, "tess.hlsl", NULL, NULL, entry, target, 0, 0, &code, &errors);
    if (FAILED(hr)) {
        printf("Compiling %s failed: %08lx %s\n", entry, hr,
               errors ? (const char *)ID3D10Blob_GetBufferPointer(errors) : "");
        return NULL;
    }
    return code;
}

int wmain(int argc, wchar_t **argv) {
    int patches = argc > 1 ? _wtoi(argv[1]) : 1600;
    if (patches <= 0) patches = 1600;
    HMODULE compiler = LoadLibraryW(L"d3dcompiler_47.dll");
    compile_fn D3DCompileFn = compiler ? (compile_fn)GetProcAddress(compiler, "D3DCompile") : NULL;
    if (!D3DCompileFn) { printf("d3dcompiler_47 is unavailable\n"); return 1; }
    HINSTANCE instance = GetModuleHandleW(NULL);
    WNDCLASSW wc = {0};
    wc.lpfnWndProc = DefWindowProcW; wc.hInstance = instance; wc.lpszClassName = L"SojuTessCheck";
    RegisterClassW(&wc);
    HWND window = CreateWindowW(wc.lpszClassName, L"Soju tessellation check",
        WS_OVERLAPPEDWINDOW | WS_VISIBLE, 180, 140, 640, 420, NULL, NULL, instance, NULL);
    DXGI_SWAP_CHAIN_DESC desc = {0};
    desc.BufferDesc.Width = 640; desc.BufferDesc.Height = 420;
    desc.BufferDesc.Format = DXGI_FORMAT_R8G8B8A8_UNORM; desc.SampleDesc.Count = 1;
    desc.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT; desc.BufferCount = 2;
    desc.OutputWindow = window; desc.Windowed = TRUE; desc.SwapEffect = DXGI_SWAP_EFFECT_DISCARD;
    D3D_FEATURE_LEVEL level = D3D_FEATURE_LEVEL_11_0;
    ID3D11Device *device = NULL; ID3D11DeviceContext *context = NULL; IDXGISwapChain *swap = NULL;
    HRESULT hr = D3D11CreateDeviceAndSwapChain(NULL, D3D_DRIVER_TYPE_HARDWARE, NULL, 0, &level, 1,
        D3D11_SDK_VERSION, &desc, &swap, &device, NULL, &context);
    if (FAILED(hr)) { printf("Device/swapchain failed: %08lx\n", hr); return 2; }
    if (!GetModuleHandleW(L"winemetal.dll")) { printf("DXMT (winemetal) is not loaded\n"); return 3; }

    ID3DBlob *vsb = compile(D3DCompileFn, "vs", "vs_5_0"), *hsb = compile(D3DCompileFn, "hs", "hs_5_0");
    ID3DBlob *dsb = compile(D3DCompileFn, "ds", "ds_5_0"), *psb = compile(D3DCompileFn, "ps", "ps_5_0");
    if (!vsb || !hsb || !dsb || !psb) return 4;
    ID3D11VertexShader *vs; ID3D11HullShader *hs; ID3D11DomainShader *ds; ID3D11PixelShader *ps;
    if (FAILED(ID3D11Device_CreateVertexShader(device, ID3D10Blob_GetBufferPointer(vsb), ID3D10Blob_GetBufferSize(vsb), NULL, &vs)) ||
        FAILED(ID3D11Device_CreateHullShader(device, ID3D10Blob_GetBufferPointer(hsb), ID3D10Blob_GetBufferSize(hsb), NULL, &hs)) ||
        FAILED(ID3D11Device_CreateDomainShader(device, ID3D10Blob_GetBufferPointer(dsb), ID3D10Blob_GetBufferSize(dsb), NULL, &ds)) ||
        FAILED(ID3D11Device_CreatePixelShader(device, ID3D10Blob_GetBufferPointer(psb), ID3D10Blob_GetBufferSize(psb), NULL, &ps))) {
        printf("Shader creation failed\n"); return 5;
    }
    ID3D11Texture2D *buffer = NULL; ID3D11RenderTargetView *target = NULL;
    if (FAILED(IDXGISwapChain_GetBuffer(swap, 0, &IID_ID3D11Texture2D, (void **)&buffer)) ||
        FAILED(ID3D11Device_CreateRenderTargetView(device, (ID3D11Resource *)buffer, NULL, &target))) return 6;
    D3D11_VIEWPORT viewport = {0, 0, 640, 420, 0, 1};
    D3D11_QUERY_DESC qd = {D3D11_QUERY_EVENT, 0};
    ID3D11Query *done; ID3D11Device_CreateQuery(device, &qd, &done);
    LARGE_INTEGER freq, start, end; QueryPerformanceFrequency(&freq);
    double total = 0; int measured = 0;
    for (int frame = 0; frame < 40; frame++) {
        MSG message;
        while (PeekMessageW(&message, NULL, 0, 0, PM_REMOVE)) { TranslateMessage(&message); DispatchMessageW(&message); }
        QueryPerformanceCounter(&start);
        float clear[4] = {0.1f, 0.1f, 0.15f, 1};
        ID3D11DeviceContext_ClearRenderTargetView(context, target, clear);
        ID3D11DeviceContext_OMSetRenderTargets(context, 1, &target, NULL);
        ID3D11DeviceContext_RSSetViewports(context, 1, &viewport);
        ID3D11DeviceContext_IASetInputLayout(context, NULL);
        ID3D11DeviceContext_IASetPrimitiveTopology(context, D3D11_PRIMITIVE_TOPOLOGY_3_CONTROL_POINT_PATCHLIST);
        ID3D11DeviceContext_VSSetShader(context, vs, NULL, 0);
        ID3D11DeviceContext_HSSetShader(context, hs, NULL, 0);
        ID3D11DeviceContext_DSSetShader(context, ds, NULL, 0);
        ID3D11DeviceContext_PSSetShader(context, ps, NULL, 0);
        ID3D11DeviceContext_Draw(context, 3 * patches, 0);
        ID3D11DeviceContext_End(context, (ID3D11Asynchronous *)done);
        hr = IDXGISwapChain_Present(swap, 0, 0);
        if (FAILED(hr)) { printf("Present failed: %08lx\n", hr); return 7; }
        while (ID3D11DeviceContext_GetData(context, (ID3D11Asynchronous *)done, NULL, 0, 0) == S_FALSE) Sleep(0);
        QueryPerformanceCounter(&end);
        if (frame >= 10) { total += (double)(end.QuadPart - start.QuadPart) * 1000.0 / freq.QuadPart; measured++; }
    }
    printf("PASS: %d tessellated frames of %d patches, average %.2f ms per frame\n", measured, patches, total / measured);
    return 0;
}
