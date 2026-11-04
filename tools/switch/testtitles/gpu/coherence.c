#include "../common.h"
#include <deko3d.h>

int main(void) {
    DkDeviceMaker device_maker;
    dkDeviceMakerDefaults(&device_maker);
    DkDevice device = dkDeviceCreate(&device_maker);
    if (!check("device", 0, device != NULL)) return finish();
    DkQueueMaker queue_maker;
    dkQueueMakerDefaults(&queue_maker, device);
    DkQueue queue = dkQueueCreate(&queue_maker);
    DkCmdBufMaker command_maker;
    dkCmdBufMakerDefaults(&command_maker, device);
    DkCmdBuf commands = dkCmdBufCreate(&command_maker);
    DkMemBlockMaker memory_maker;
    dkMemBlockMakerDefaults(&memory_maker, device, 65536);
    DkMemBlock command_memory = dkMemBlockCreate(&memory_maker);
    dkCmdBufAddMemory(commands, command_memory, 0, 65536);
    DkImageLayoutMaker layout_maker;
    dkImageLayoutMakerDefaults(&layout_maker, device);
    layout_maker.flags = DkImageFlags_UsageRender;
    layout_maker.format = DkImageFormat_RGBA8_Unorm;
    layout_maker.dimensions[0] = 32;
    layout_maker.dimensions[1] = 32;
    DkImageLayout layout;
    dkImageLayoutInitialize(&layout, &layout_maker);
    uint32_t image_size = (uint32_t)dkImageLayoutGetSize(&layout);
    dkMemBlockMakerDefaults(&memory_maker, device, (image_size + 8191) & ~4095u);
    memory_maker.flags |= DkMemBlockFlags_Image;
    DkMemBlock image_memory = dkMemBlockCreate(&memory_maker);
    DkImage image;
    dkImageInitialize(&image, &layout, image_memory, 0);
    DkImageView view;
    dkImageViewDefaults(&view, &image);
    const DkImageView *views[] = {&view};
    volatile uint32_t *cpu = dkMemBlockGetCpuAddr(image_memory);
    cpu[image_size / 4] = 0x12345678;
    for (unsigned pass = 0; pass < 2; pass++) {
        cpu[0] = 0x11223344;
        dkCmdBufBindRenderTargets(commands, views, 1, NULL);
        dkCmdBufClearColorFloat(commands, 0, 15, pass ? 0 : 1, pass ? 1 : 0, 0, 1);
        dkQueueSubmitCommands(queue, dkCmdBufFinishList(commands));
        dkQueueWaitIdle(queue);
        const uint32_t expected = pass ? 0xff00ff00 : 0xff0000ff;
        bool visible = true;
        for (unsigned word = 0; word < 32; word++) visible &= cpu[word] == expected;
        check(pass ? "CPU reads second GPU clear after fence" : "CPU reads first GPU clear after fence", 0, visible);
        check("GPU write preserves adjacent CPU guard", 0, cpu[image_size / 4] == 0x12345678);
        dkCmdBufClear(commands);
        dkCmdBufAddMemory(commands, command_memory, 0, 65536);
    }
    dkQueueDestroy(queue);
    dkCmdBufDestroy(commands);
    dkMemBlockDestroy(image_memory);
    dkMemBlockDestroy(command_memory);
    dkDeviceDestroy(device);
    return finish();
}
