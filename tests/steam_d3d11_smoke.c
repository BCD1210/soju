#define COBJMACROS
#include <windows.h>
#include <d3d11.h>
#include <stdio.h>

/* Hardware rendering smoke test. No store account or installed game is needed. */
int wmain(void) {
    HINSTANCE instance = GetModuleHandleW(NULL);
    WNDCLASSW wc = {0};
    wc.lpfnWndProc = DefWindowProcW;
    wc.hInstance = instance;
    wc.lpszClassName = L"SojuGraphicsCheck";
    RegisterClassW(&wc);
    HWND window = CreateWindowW(wc.lpszClassName, L"Soju D3D11 rendering check",
        WS_OVERLAPPEDWINDOW | WS_VISIBLE, 180, 140, 640, 420, NULL, NULL, instance, NULL);
    if (!window) return 1;
    DXGI_SWAP_CHAIN_DESC desc = {0};
    desc.BufferDesc.Width = 640; desc.BufferDesc.Height = 420;
    desc.BufferDesc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
    desc.SampleDesc.Count = 1;
    desc.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
    desc.BufferCount = 2; desc.OutputWindow = window; desc.Windowed = TRUE;
    desc.SwapEffect = DXGI_SWAP_EFFECT_DISCARD;
    ID3D11Device *device = NULL;
    ID3D11DeviceContext *context = NULL;
    IDXGISwapChain *swap = NULL;
    HRESULT hr = D3D11CreateDeviceAndSwapChain(NULL, D3D_DRIVER_TYPE_HARDWARE, NULL,
        0, NULL, 0, D3D11_SDK_VERSION, &desc, &swap, &device, NULL, &context);
    if (FAILED(hr)) { printf("Device/swapchain failed: %08lx\n", hr); return 2; }
    if (!GetModuleHandleW(L"winemetal.dll")) { printf("DXMT winemetal module not loaded\n"); return 6; }
    ID3D11Texture2D *buffer = NULL;
    ID3D11RenderTargetView *target = NULL;
    hr = IDXGISwapChain_GetBuffer(swap, 0, &IID_ID3D11Texture2D, (void **)&buffer);
    if (FAILED(hr)) return 3;
    hr = ID3D11Device_CreateRenderTargetView(device, (ID3D11Resource *)buffer, NULL, &target);
    if (FAILED(hr)) return 4;
    ID3D11Texture2D_Release(buffer);
    for (int frame = 0; frame < 180; frame++) {
        MSG message;
        while (PeekMessageW(&message, NULL, 0, 0, PM_REMOVE)) {
            TranslateMessage(&message); DispatchMessageW(&message);
        }
        float color[4] = {0.12f, 0.5f + 0.2f * (frame % 60) / 60, 0.35f, 1};
        ID3D11DeviceContext_ClearRenderTargetView(context, target, color);
        hr = IDXGISwapChain_Present(swap, 1, 0);
        if (FAILED(hr)) { printf("Present failed: %08lx\n", hr); return 5; }
        Sleep(33);
    }
    printf("PASS: hardware D3D11 device, swapchain and 180 presented frames.\n");
    ID3D11RenderTargetView_Release(target); IDXGISwapChain_Release(swap);
    ID3D11DeviceContext_Release(context); ID3D11Device_Release(device);
    DestroyWindow(window);
    return 0;
}
