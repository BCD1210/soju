#include "common.h"
u32 __nx_applet_type = AppletType_Application;
int main(void) {
    __nx_applet_type = AppletType_Application;
    if (!CALL(appletInitialize())) return finish();
    check("docked operation mode", 0, appletGetOperationMode() == AppletOperationMode_Console);
    check("docked performance mode", 0, appletGetPerformanceMode() == ApmPerformanceMode_Boost);
    if (!CALL(nvInitialize())) return finish();
    if (!CALL(viInitialize(ViServiceType_Application))) return finish();
    ViDisplay display; ViLayer layer; NWindow native;
    if (!CALL(viOpenDefaultDisplay(&display))) return finish();
    if (!CALL(viCreateLayer(&display, &layer))) return finish();
    if (!CALL(nwindowCreateFromLayer(&native, &layer))) return finish();
    NWindow *window = &native;
    for (unsigned i=0; i<2; i++) {
        u32 width = i ? 1920 : 1280, height = i ? 1080 : 720;
        Framebuffer fb;
        if (!CALL(framebufferCreate(&fb, window, width, height, PIXEL_FORMAT_RGBA_8888, 2))) break;
        CALL(framebufferMakeLinear(&fb));
        u32 stride = 0;
        u32 *pixels = framebufferBegin(&fb, &stride);
        check(i ? "dequeue 1920x1080" : "dequeue 1280x720", 0, pixels && stride >= width*4);
        if (pixels) {
            for (u32 y=0; y<height; y++) for (u32 x=0; x<width; x++)
                pixels[y*(stride/4)+x] = x < width/2 ? 0xff332211 : 0xffccbbaa;
            framebufferEnd(&fb);
            check(i ? "queue 1920x1080" : "queue 1280x720", 0, true);
        }
        framebufferClose(&fb);
    }
    const AudioRendererConfig config = {
        .output_rate = AudioRendererOutputRate_48kHz, .num_voices = 1,
        .num_effects = 0, .num_sinks = 1, .num_mix_objs = 1, .num_mix_buffers = 2,
    };
    if (CALL(audrenInitialize(&config))) {
        AudioDriver driver;
        if (CALL(audrvCreate(&driver, &config, 2))) {
            const u8 channels[] = {0, 1};
            int sink = audrvDeviceSinkAdd(&driver, AUDREN_DEFAULT_DEVICE_NAME, 2, channels);
            check("audio stereo sink", 0, sink >= 0);
            CALL(audrvUpdate(&driver));
            CALL(audrenStartAudioRenderer());
            CALL(audrenStopAudioRenderer());
            audrvClose(&driver);
        }
        audrenExit();
    }
    nwindowClose(window);
    CALL(viCloseLayer(&layer));
    CALL(viCloseDisplay(&display));
    viExit();
    nvExit();
    appletExit();
    return finish();
}
