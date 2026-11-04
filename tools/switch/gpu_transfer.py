"""Bounded linear GPU transfers for a future command executor.

The caller must resolve GPU virtual addresses and validate full ranges against
live channel mappings. Tiled layouts, conversion, reduction and overlap are
not implemented. This module does not submit GPU commands or signal fences.
"""

MAX_COPY_BYTES = 8 * 1024 * 1024
MAX_ROWS = 4096


def plan_linear_copy(src, dst, row_bytes, rows, src_pitch, dst_pitch, read_gpu, check_write):
    """Snapshot a non-overlapping pitched copy after validating every row."""
    if (min(src, dst) < 0 or row_bytes <= 0 or rows <= 0 or rows > MAX_ROWS
            or row_bytes * rows > MAX_COPY_BYTES
            or src_pitch < row_bytes or dst_pitch < row_bytes):
        raise ValueError('invalid linear GPU copy geometry')
    src_end = src + (rows - 1) * src_pitch + row_bytes
    dst_end = dst + (rows - 1) * dst_pitch + row_bytes
    if src_end > 1 << 40 or dst_end > 1 << 40:
        raise ValueError('GPU copy address overflow')
    if src < dst_end and dst < src_end:
        raise ValueError('overlapping GPU copies are unsupported')
    plan = []
    for row in range(rows):
        source = src + row * src_pitch
        target = dst + row * dst_pitch
        if not check_write(target, row_bytes):
            raise ValueError('GPU copy destination is not writable')
        data = bytes(read_gpu(source, row_bytes))
        if len(data) != row_bytes:
            raise ValueError('GPU copy source is truncated')
        plan.append((target, data))
    return plan


def commit_linear_copy(plan, write_gpu):
    """Apply a fully validated plan; caller guarantees writable mappings stay live."""
    for address, data in plan:
        write_gpu(address, data)
