#import <Foundation/Foundation.h>
#import <GameController/GameController.h>
#include <SDL2/SDL.h>

static SDL_Window *window;
static SDL_Renderer *renderer;
static SDL_Texture *texture;
static int width, height;
static Uint64 pressed_until[SDL_NUM_SCANCODES];
static Uint64 pressed_frame[SDL_NUM_SCANCODES];
static Uint64 frame_number;

void soju_host_close(void) {
    SDL_DestroyTexture(texture); texture = NULL;
    SDL_DestroyRenderer(renderer); renderer = NULL;
    SDL_DestroyWindow(window); window = NULL;
    [GCController stopWirelessControllerDiscovery];
    SDL_Quit();
}
const char *soju_host_error(void) { return SDL_GetError(); }
int soju_host_open(void) {
    memset(pressed_until, 0, sizeof(pressed_until));
    memset(pressed_frame, 0, sizeof(pressed_frame));
    frame_number = 0;
    SDL_SetMainReady();
    if (SDL_Init(SDL_INIT_VIDEO | SDL_INIT_EVENTS)) return -1;
    window = SDL_CreateWindow("Soju — Switch homebrew", SDL_WINDOWPOS_CENTERED,
        SDL_WINDOWPOS_CENTERED, 1280, 720, SDL_WINDOW_RESIZABLE | SDL_WINDOW_ALLOW_HIGHDPI);
    if (!window) return -1;
    renderer = SDL_CreateRenderer(window, -1, SDL_RENDERER_ACCELERATED | SDL_RENDERER_PRESENTVSYNC);
    if (!renderer) return -1;
    [GCController startWirelessControllerDiscoveryWithCompletionHandler:nil];
    return 0;
}
int soju_host_present(const unsigned char *pixels, int w, int h) {
    if (!texture || w != width || h != height) {
        SDL_DestroyTexture(texture);
        texture = SDL_CreateTexture(renderer, SDL_PIXELFORMAT_RGBA32, SDL_TEXTUREACCESS_STREAMING, w, h);
        if (!texture) return -1;
        width = w; height = h;
        if (SDL_RenderSetLogicalSize(renderer, w, h)) return -1;
    }
    if (SDL_UpdateTexture(texture, NULL, pixels, w * 4)) return -1;
    if (SDL_RenderClear(renderer) || SDL_RenderCopy(renderer, texture, NULL, NULL)) return -1;
    SDL_RenderPresent(renderer);
    ++frame_number;
    return 0;
}
// Flat arrays keep the Objective-C/Rust ABI independent of compiler bool layout.
int soju_host_poll(unsigned char *keys, float *pad) {
    @autoreleasepool {
        SDL_Event event;
        int quit = 0;
        while (SDL_PollEvent(&event)) {
            if (event.type == SDL_KEYDOWN && !event.key.repeat) {
                pressed_until[event.key.keysym.scancode] = SDL_GetTicks64() + 500;
                pressed_frame[event.key.keysym.scancode] = frame_number + 2;
            }
            if (event.type == SDL_WINDOWEVENT && event.window.event == SDL_WINDOWEVENT_FOCUS_LOST) memset(pressed_until, 0, sizeof(pressed_until));
            if (event.type == SDL_QUIT || (event.type == SDL_KEYDOWN && event.key.keysym.sym == SDLK_ESCAPE)) quit = 1;
        }
        memset(keys, 0, 128);
        memset(pad, 0, 22 * sizeof(float));
        if (!(SDL_GetWindowFlags(window) & SDL_WINDOW_INPUT_FOCUS)) return quit;
        const Uint8 *state = SDL_GetKeyboardState(NULL);
        const int pairs[][2] = {
            {SDL_SCANCODE_A,0x00},{SDL_SCANCODE_S,0x01},{SDL_SCANCODE_D,0x02},
            {SDL_SCANCODE_Q,0x0c},{SDL_SCANCODE_W,0x0d},{SDL_SCANCODE_E,0x0e},
            {SDL_SCANCODE_Z,0x06},{SDL_SCANCODE_B,0x06},{SDL_SCANCODE_X,0x07},
            {SDL_SCANCODE_C,0x08},{SDL_SCANCODE_V,0x09},{SDL_SCANCODE_I,0x22},
            {SDL_SCANCODE_J,0x26},{SDL_SCANCODE_K,0x28},{SDL_SCANCODE_L,0x25},
            {SDL_SCANCODE_RETURN,0x24},{SDL_SCANCODE_TAB,0x30},{SDL_SCANCODE_SPACE,0x31},
            {SDL_SCANCODE_BACKSPACE,0x33},{SDL_SCANCODE_1,0x12},{SDL_SCANCODE_3,0x14},
            {SDL_SCANCODE_LEFT,0x7b},{SDL_SCANCODE_RIGHT,0x7c},{SDL_SCANCODE_DOWN,0x7d},{SDL_SCANCODE_UP,0x7e}
        };
        // Keep quick taps until the guest can scan input, including on a slow frame.
        for (unsigned i = 0; i < sizeof(pairs)/sizeof(pairs[0]); ++i) {
            int scancode = pairs[i][0];
            bool pulse = pressed_until[scancode] > SDL_GetTicks64() && pressed_frame[scancode] > frame_number;
            keys[pairs[i][1]] |= state[scancode] || pulse;
        }
        for (GCController *controller in GCController.controllers) {
            GCExtendedGamepad *g = controller.extendedGamepad;
            if (!g) continue;
            pad[0]=g.buttonA.value; pad[1]=g.buttonB.value; pad[2]=g.buttonX.value; pad[3]=g.buttonY.value;
            pad[4]=g.leftShoulder.value; pad[5]=g.rightShoulder.value;
            pad[6]=g.leftTrigger.value; pad[7]=g.rightTrigger.value;
            pad[8]=g.leftThumbstickButton.value; pad[9]=g.rightThumbstickButton.value;
            pad[10]=g.buttonMenu.value; pad[11]=g.buttonOptions.value;
            pad[12]=g.dpad.up.value; pad[13]=g.dpad.down.value; pad[14]=g.dpad.left.value; pad[15]=g.dpad.right.value;
            pad[16]=g.leftThumbstick.xAxis.value; pad[17]=g.leftThumbstick.yAxis.value;
            pad[18]=g.rightThumbstick.xAxis.value; pad[19]=g.rightThumbstick.yAxis.value;
            break;
        }
        return quit;
    }
}

#import <AudioToolbox/AudioToolbox.h>
#include <stdatomic.h>
#define AUDIO_CAPACITY 96000
// Single producer (runtime) and single consumer (AudioUnit render callback).
typedef struct {
    AudioUnit unit;
    int16_t samples[AUDIO_CAPACITY];
    _Atomic uint64_t read;
    _Atomic uint64_t write;
    _Atomic uint64_t nonzero;
} SojuAudio;
static OSStatus audio_render(void *context, AudioUnitRenderActionFlags *flags,
    const AudioTimeStamp *time, UInt32 bus, UInt32 frames, AudioBufferList *data) {
    (void)flags; (void)time; (void)bus;
    SojuAudio *audio = context;
    for (UInt32 i = 0; i < data->mNumberBuffers; ++i) {
        if (data->mBuffers[i].mData) memset(data->mBuffers[i].mData, 0, data->mBuffers[i].mDataByteSize);
    }
    if (data->mNumberBuffers != 1 || !data->mBuffers[0].mData) return noErr;
    uint64_t read = atomic_load_explicit(&audio->read, memory_order_relaxed);
    uint64_t write = atomic_load_explicit(&audio->write, memory_order_acquire);
    size_t available = MIN((uint64_t)data->mBuffers[0].mDataByteSize / 2, (uint64_t)frames * 2);
    size_t n = MIN(available, write - read);
    int16_t *out = data->mBuffers[0].mData;
    uint64_t nonzero = 0;
    for (size_t i = 0; i < n; ++i) {
        out[i] = audio->samples[(read+i) % AUDIO_CAPACITY];
        nonzero += out[i] != 0;
    }
    atomic_store_explicit(&audio->read, read + n, memory_order_release);
    atomic_fetch_add_explicit(&audio->nonzero, nonzero, memory_order_relaxed);
    return noErr;
}
void soju_audio_close(SojuAudio *audio) {
    if (!audio) return;
    if (audio->unit) {
        AudioOutputUnitStop(audio->unit);
        AudioUnitUninitialize(audio->unit);
        AudioComponentInstanceDispose(audio->unit);
    }
    free(audio);
}
SojuAudio *soju_audio_open(int32_t *error) {
    SojuAudio *audio = calloc(1, sizeof(SojuAudio));
    if (!audio) { *error = -108; return NULL; }
    atomic_init(&audio->read, 0); atomic_init(&audio->write, 0); atomic_init(&audio->nonzero, 0);
    AudioComponentDescription description = { kAudioUnitType_Output, kAudioUnitSubType_DefaultOutput, kAudioUnitManufacturer_Apple, 0, 0 };
    AudioComponent component = AudioComponentFindNext(NULL, &description);
    if (!component) { *error = -1; soju_audio_close(audio); return NULL; }
    *error = AudioComponentInstanceNew(component, &audio->unit);
    if (*error) { soju_audio_close(audio); return NULL; }
    AudioStreamBasicDescription format = { .mSampleRate=48000, .mFormatID=kAudioFormatLinearPCM,
        .mFormatFlags=kAudioFormatFlagIsSignedInteger | kAudioFormatFlagIsPacked,
        .mBytesPerPacket=4, .mFramesPerPacket=1, .mBytesPerFrame=4, .mChannelsPerFrame=2, .mBitsPerChannel=16 };
    *error = AudioUnitSetProperty(audio->unit, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Input, 0, &format, sizeof(format));
    if (*error) { soju_audio_close(audio); return NULL; }
    AURenderCallbackStruct callback = { audio_render, audio };
    *error = AudioUnitSetProperty(audio->unit, kAudioUnitProperty_SetRenderCallback, kAudioUnitScope_Input, 0, &callback, sizeof(callback));
    if (*error) { soju_audio_close(audio); return NULL; }
    *error = AudioUnitInitialize(audio->unit);
    if (*error) { soju_audio_close(audio); return NULL; }
    *error = AudioOutputUnitStart(audio->unit);
    if (*error) { soju_audio_close(audio); return NULL; }
    return audio;
}
uint64_t soju_audio_queued(SojuAudio *audio) {
    uint64_t read = atomic_load_explicit(&audio->read, memory_order_acquire);
    return atomic_load_explicit(&audio->write, memory_order_relaxed) - read;
}
int soju_audio_write(SojuAudio *audio, const int16_t *samples, size_t count) {
    uint64_t write = atomic_load_explicit(&audio->write, memory_order_relaxed);
    uint64_t read = atomic_load_explicit(&audio->read, memory_order_acquire);
    if (count > AUDIO_CAPACITY - (write-read)) return -1;
    for (size_t i = 0; i < count; ++i) audio->samples[(write+i) % AUDIO_CAPACITY] = samples[i];
    atomic_store_explicit(&audio->write, write + count, memory_order_release);
    return 0;
}
uint64_t soju_audio_nonzero(SojuAudio *audio) { return atomic_load_explicit(&audio->nonzero, memory_order_relaxed); }
