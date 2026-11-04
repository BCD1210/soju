#import <Foundation/Foundation.h>
#import <GameController/GameController.h>
#include <SDL2/SDL.h>

static SDL_Window *window;
static SDL_Renderer *renderer;
static SDL_Texture *texture;
static int width, height;
static Uint64 pressed_until[SDL_NUM_SCANCODES];

void soju_host_close(void) {
    SDL_DestroyTexture(texture); texture = NULL;
    SDL_DestroyRenderer(renderer); renderer = NULL;
    SDL_DestroyWindow(window); window = NULL;
    SDL_Quit();
}
const char *soju_host_error(void) { return SDL_GetError(); }
int soju_host_open(void) {
    SDL_SetMainReady();
    if (SDL_Init(SDL_INIT_VIDEO | SDL_INIT_EVENTS)) return -1;
    window = SDL_CreateWindow("Soju — Switch homebrew", SDL_WINDOWPOS_CENTERED,
        SDL_WINDOWPOS_CENTERED, 1280, 720, SDL_WINDOW_RESIZABLE | SDL_WINDOW_ALLOW_HIGHDPI);
    if (!window) return -1;
    renderer = SDL_CreateRenderer(window, -1, SDL_RENDERER_ACCELERATED);
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
    return 0;
}
// Flat arrays keep the Objective-C/Rust ABI independent of compiler bool layout.
int soju_host_poll(unsigned char *keys, float *pad) {
    @autoreleasepool {
        SDL_Event event;
        int quit = 0;
        while (SDL_PollEvent(&event)) {
            if (event.type == SDL_KEYDOWN && !event.key.repeat) pressed_until[event.key.keysym.scancode] = SDL_GetTicks64() + 80;
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
        for (unsigned i = 0; i < sizeof(pairs)/sizeof(pairs[0]); ++i) keys[pairs[i][1]] |= state[pairs[i][0]] || pressed_until[pairs[i][0]] > SDL_GetTicks64();
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
