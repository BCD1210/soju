#import <Foundation/Foundation.h>
#import <Metal/Metal.h>
#import <QuartzCore/CAMetalLayer.h>
#include <stdatomic.h>

// Main-thread uploads own one slot until its command buffer completes.
static id<MTLDevice> device;
static id<MTLCommandQueue> commands;
static id<MTLRenderPipelineState> pipeline;
static CAMetalLayer *surface;
static dispatch_queue_t encoding;
static dispatch_semaphore_t available[3];
static id<MTLTexture> uploads[3];
static unsigned next_slot;
static _Atomic bool failed;

int soju_presenter_drain(void) {
    if (!encoding) return 0;
    dispatch_sync(encoding, ^{});
    for (unsigned i = 0; i < 3; ++i) {
        dispatch_semaphore_wait(available[i], DISPATCH_TIME_FOREVER);
        dispatch_semaphore_signal(available[i]);
    }
    return atomic_load_explicit(&failed, memory_order_acquire) ? -1 : 0;
}
void soju_presenter_close(void) {
    soju_presenter_drain();
    for (unsigned i = 0; i < 3; ++i) { uploads[i] = nil; available[i] = nil; }
    encoding = nil; pipeline = nil; commands = nil; surface = nil; device = nil;
}
int soju_presenter_open(CAMetalLayer *layer) {
    if (encoding || !layer) return -1;
    device = MTLCreateSystemDefaultDevice();
    commands = [device newCommandQueue];
    NSString *source = @"#include <metal_stdlib>\nusing namespace metal;\n"
        "struct V { float4 p [[position]]; float2 uv; };\n"
        "vertex V present_vertex(uint id [[vertex_id]],constant float2& scale [[buffer(0)]]) {\n"
        "float2 p[6]={float2(-1,-1),float2(1,-1),float2(1,1),float2(-1,-1),float2(1,1),float2(-1,1)};\n"
        "V o; o.p=float4(p[id]*scale,0,1); o.uv=float2(p[id].x+1,1-p[id].y)*0.5; return o; }\n"
        "fragment float4 present_fragment(V in [[stage_in]],texture2d<float> image [[texture(0)]]) {\n"
        "constexpr sampler s(coord::normalized,address::clamp_to_edge,filter::linear);return image.sample(s,in.uv);}\n";
    NSError *error = nil;
    id<MTLLibrary> library = [device newLibraryWithSource:source options:nil error:&error];
    MTLRenderPipelineDescriptor *descriptor = [MTLRenderPipelineDescriptor new];
    descriptor.vertexFunction = [library newFunctionWithName:@"present_vertex"];
    descriptor.fragmentFunction = [library newFunctionWithName:@"present_fragment"];
    descriptor.colorAttachments[0].pixelFormat = MTLPixelFormatBGRA8Unorm;
    if (library) pipeline = [device newRenderPipelineStateWithDescriptor:descriptor error:&error];
    if (!device || !commands || !pipeline) { soju_presenter_close(); return -1; }
    surface = layer;
    surface.device = device;
    surface.pixelFormat = MTLPixelFormatBGRA8Unorm;
    surface.framebufferOnly = YES;
    surface.maximumDrawableCount = 3;
    surface.displaySyncEnabled = YES;
    surface.allowsNextDrawableTimeout = YES;
    encoding = dispatch_queue_create("soju.presentation", DISPATCH_QUEUE_SERIAL);
    for (unsigned i = 0; i < 3; ++i) available[i] = dispatch_semaphore_create(1);
    next_slot = 0;
    atomic_store_explicit(&failed, false, memory_order_release);
    return 0;
}
int soju_presenter_submit(const unsigned char *pixels, int width, int height) {
    if (!encoding || !pixels || width < 1 || height < 1 || width > 4096 || height > 2160) return -1;
    unsigned slot = next_slot;
    dispatch_semaphore_t ready = available[slot];
    dispatch_semaphore_wait(ready, DISPATCH_TIME_FOREVER);
    if (atomic_load_explicit(&failed, memory_order_acquire)) {
        dispatch_semaphore_signal(ready); return -1;
    }
    id<MTLTexture> upload = uploads[slot];
    if (!upload || upload.width != (NSUInteger)width || upload.height != (NSUInteger)height) {
        MTLTextureDescriptor *descriptor = [MTLTextureDescriptor texture2DDescriptorWithPixelFormat:MTLPixelFormatRGBA8Unorm width:width height:height mipmapped:NO];
        descriptor.storageMode = MTLStorageModeShared;
        descriptor.usage = MTLTextureUsageShaderRead;
        upload = [device newTextureWithDescriptor:descriptor];
        if (!upload) { dispatch_semaphore_signal(ready); return -1; }
        uploads[slot] = upload;
    }
    [upload replaceRegion:MTLRegionMake2D(0,0,width,height) mipmapLevel:0 withBytes:pixels bytesPerRow:(NSUInteger)width*4];
    next_slot = (slot + 1) % 3;
    dispatch_async(encoding, ^{
        @autoreleasepool {
            id<CAMetalDrawable> drawable = [surface nextDrawable];
            // Minimized/occluded surfaces can temporarily have no drawable.
            if (!drawable) { dispatch_semaphore_signal(ready); return; }
            id<MTLCommandBuffer> command = [commands commandBuffer];
            MTLRenderPassDescriptor *pass = [MTLRenderPassDescriptor renderPassDescriptor];
            pass.colorAttachments[0].texture = drawable.texture;
            pass.colorAttachments[0].loadAction = MTLLoadActionClear;
            pass.colorAttachments[0].storeAction = MTLStoreActionStore;
            pass.colorAttachments[0].clearColor = MTLClearColorMake(0,0,0,1);
            id<MTLRenderCommandEncoder> encoder = [command renderCommandEncoderWithDescriptor:pass];
            if (!command || !encoder) {
                atomic_store_explicit(&failed, true, memory_order_release);
                dispatch_semaphore_signal(ready); return;
            }
            float image_aspect = (float)width/height;
            float view_aspect = (float)drawable.texture.width/drawable.texture.height;
            float scale[2] = { fminf(1,image_aspect/view_aspect), fminf(1,view_aspect/image_aspect) };
            [encoder setRenderPipelineState:pipeline];
            [encoder setVertexBytes:scale length:sizeof(scale) atIndex:0];
            [encoder setFragmentTexture:upload atIndex:0];
            [encoder drawPrimitives:MTLPrimitiveTypeTriangle vertexStart:0 vertexCount:6];
            [encoder endEncoding];
            [command presentDrawable:drawable];
            [command addCompletedHandler:^(id<MTLCommandBuffer> completed) {
                if (completed.status != MTLCommandBufferStatusCompleted)
                    atomic_store_explicit(&failed, true, memory_order_release);
                dispatch_semaphore_signal(ready);
            }];
            [command commit];
        }
    });
    return 0;
}
