/* Block-linear RGBA/BGRA conversion. The caller validates all bounds. */
#include <stdint.h>
#include <stddef.h>
#include <string.h>

int soju_switch_convert(const uint8_t *src, size_t src_size, uint8_t *dst,
                        size_t dst_size, uint32_t width, uint32_t height,
                        uint32_t pitch, uint32_t log2_height, uint32_t format) {
    if (!src || !dst || !width || !height || width > 4096 || height > 2160 ||
        pitch < width * 4 || pitch > 65536 || pitch % 64 || log2_height > 5 ||
        (format != 1 && format != 5)) return -1;
    const size_t bh = (size_t)1 << log2_height, block_rows = 8 * bh;
    const size_t needed = (size_t)pitch * ((height + block_rows - 1) / block_rows) * block_rows;
    if (needed > src_size || (size_t)width * height * 4 > dst_size) return -1;
    for (size_t y = 0; y < height; ++y) {
        const size_t base = (y / block_rows) * pitch * block_rows + ((y / 8) % bh) * 512 +
                            ((y % 8) / 2) * 64 + (y % 2) * 16;
        for (size_t x = 0; x < (size_t)width * 4; x += 16) {
            const size_t off = base + (x / 64) * 512 * bh + ((x % 64) / 32) * 256 +
                               ((x % 32) / 16) * 32;
            const size_t n = (size_t)width * 4 - x < 16 ? (size_t)width * 4 - x : 16;
            if (off > src_size || n > src_size - off) return -1;
            memcpy(dst + y * width * 4 + x, src + off, n);
        }
    }
    if (format == 5) {
        for (size_t i = 0; i < (size_t)width * height * 4; i += 4) {
            const uint8_t tmp = dst[i];
            dst[i] = dst[i + 2];
            dst[i + 2] = tmp;
        }
    }
    return 0;
}
