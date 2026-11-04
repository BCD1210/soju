// Standalone native ownership/resize/lifecycle checks; no guest assets are needed.
#import "host.m"
#import "presenter.m"
#include <assert.h>

int main(void) {
    @autoreleasepool {
        assert(soju_presenter_open(nil) == -1);
        for (unsigned cycle=0; cycle<2; ++cycle) {
            assert(soju_host_open() == 0);
            unsigned expected[3] = {0};
            for (unsigned frame=0; frame<48; ++frame) {
                int w = frame%2 ? 193 : 257, h = frame%3 ? 129 : 181;
                size_t bytes = (size_t)w*h*4;
                unsigned char *pixels = malloc(bytes);
                memset(pixels,frame+1,bytes);
                expected[next_slot] = frame+1;
                assert(soju_host_present(pixels,w,h) == 0);
                // The API must own the submitted bytes before returning.
                memset(pixels,0,bytes); free(pixels);
                if (frame%12 == 0) SDL_SetWindowSize(window,frame%24 ? 720 : 960,540);
                unsigned char keys[128]; float pad[22];
                soju_host_poll(keys,pad);
            }
            assert(soju_host_drain() == 0);
            for (unsigned slot=0;slot<3;++slot) {
                id<MTLTexture> texture=uploads[slot];
                size_t bytes=texture.width*texture.height*4;
                unsigned char *pixels=malloc(bytes);
                [texture getBytes:pixels bytesPerRow:texture.width*4 fromRegion:MTLRegionMake2D(0,0,texture.width,texture.height) mipmapLevel:0];
                for (size_t i=0;i<bytes;++i) assert(pixels[i] == expected[slot]);
                free(pixels);
            }
            assert(soju_presenter_submit(NULL,1,1) == -1);
            soju_host_close();
            assert(soju_presenter_submit((const unsigned char *)"x",1,1) == -1);
            assert(soju_host_drain() == 0);
        }
        puts("presentation: 96 queued frames, resize, owned uploads and reopen pass");
    }
    return 0;
}
