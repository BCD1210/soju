"""Bounded block-linear uniform color clear planning.

This virtual GPU subset supports only complete 8-row GOBs, tightly pitched
32-bit pixels, and all color channels. It preserves padded rows and never
touches depth/stencil. No writes occur until the caller commits the plan.
"""
MAX_SURFACE_BYTES = 32 * 1024 * 1024


def plan_uniform_clear(address, width, height, block_height_log2, pixel):
    if (address < 0 or address % 4 or width <= 0 or width > 4096 or width % 16
            or height <= 0 or height > 2160 or height % 8
            or not 0 <= block_height_log2 <= 5 or len(pixel) != 4):
        raise ValueError('unsupported block-linear clear geometry')
    pitch = width * 4
    block_gobs = 1 << block_height_log2
    block_rows = 8 * block_gobs
    padded_height = ((height + block_rows - 1) // block_rows) * block_rows
    allocation_span = pitch * padded_height
    if allocation_span > MAX_SURFACE_BYTES or address + allocation_span > 1 << 40:
        raise ValueError('block-linear clear exceeds limit')
    full_blocks, remaining_rows = divmod(height, block_rows)
    writes = []
    if full_blocks:
        length = full_blocks * pitch * block_rows
        writes.append((address, pixel * (length // 4)))
    if remaining_rows:
        # Height is a multiple of 8: every touched GOB is fully covered.
        gob_count = remaining_rows // 8
        group_base = address + full_blocks * pitch * block_rows
        for column in range(pitch // 64):
            target = group_base + column * block_gobs * 512
            length = gob_count * 512
            writes.append((target, pixel * (length // 4)))
    return writes


def plan_pitch_to_tiled(source, target, width, height, source_pitch, block_height_log2):
    """Return source/destination spans for a tightly described 32-bit image."""
    if (min(source, target) < 0 or width <= 0 or width > 4096 or height <= 0 or height > 2160
            or width * height * 4 > 8 * 1024 * 1024 or source_pitch < width * 4
            or not 0 <= block_height_log2 <= 5):
        raise ValueError('unsupported pitch-to-tiled geometry')
    pitch = ((width * 4 + 63) // 64) * 64
    block_gobs = 1 << block_height_log2
    block_rows = 8 * block_gobs
    span = pitch * ((height + block_rows - 1) // block_rows) * block_rows
    if target + span > 1 << 40 or source + (height - 1) * source_pitch + width * 4 > 1 << 40:
        raise ValueError('pitch-to-tiled address overflow')
    copies = []
    for y in range(height):
        base = ((y // block_rows) * pitch * block_rows + ((y // 8) % block_gobs) * 512
                + ((y % 8) // 2) * 64 + (y % 2) * 16)
        for x in range(0, width * 4, 16):
            offset = base + (x // 64) * 512 * block_gobs + ((x % 64) // 32) * 256 + ((x % 32) // 16) * 32
            copies.append((source + y * source_pitch + x, target + offset, min(16, width * 4 - x)))
            if len(copies) > 65536:
                raise ValueError('pitch-to-tiled operation count exceeds limit')
    return tuple(copies)
