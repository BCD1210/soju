"""Bounded decoder for Maxwell GPFIFO method packets.

This decodes writes; it does not execute GPU methods or render.
"""
import struct

def gpu_characteristics(request):
    """Build the bounded GM20B query reply; this does not implement rendering."""
    if len(request) != 176:
        raise ValueError('invalid GPU characteristics size')
    data = bytearray(request)
    size, sentinel = struct.unpack_from('<QQ', data)
    if size == 0 or sentinel == 0:
        raise ValueError('invalid GPU characteristics request')
    struct.pack_into('<Q', data, 0, 0xa0)
    struct.pack_into('<4I2Q', data, 0x10, 0x120, 0xb, 0xa1, 1, 0x40000, 0)
    struct.pack_into('<12I', data, 0x30, 2, 0x20, 0x20000, 0x20000, 0x1b, 0x30000, 1, 0x503, 0x503, 0x80, 0x28, 0)
    struct.pack_into('<Q', data, 0x60, 0x55)
    struct.pack_into('<14I', data, 0x68, 0x902d, 0xb197, 0xb1c0, 0xb06f, 0xa140, 0xb0b5, 1, 0, 2, 1, 0, 1, 0x21d70, 0)
    struct.pack_into('<2Q', data, 0xa0, 0x6230326d67, 0)
    return bytes(data)


def gpu_metadata_ioctl(ioctl, request, output_size):
    """Return bounded virtual-GPU metadata, without claiming GPU execution support.

    ZCull context storage is an emulator-defined 4 KiB allocation contract; it
    is not a measured GM20B firmware value. No ZCull execution is implemented.
    """
    if request:
        raise ValueError('GPU metadata ioctl must not have an input buffer')
    if ioctl == 0x80044701 and output_size == 4:
        return struct.pack('<I', 0x1000)
    if ioctl == 0x80284702 and output_size == 40:
        return struct.pack('<10I', 0x20, 0x20, 0x400, 0x800, 0x20,
                           0x20, 0xc0, 0x20, 0x40, 0x10)
    raise ValueError('unsupported GPU metadata ioctl or output size')


def inspect_submission(arguments, entries, capacity, read_gpu):
    """Validate and decode a submission snapshot without executing or signaling it."""
    if len(arguments) != 24:
        raise ValueError('invalid GPU submission argument size')
    pointer, count, flags, fence_id, fence_value = struct.unpack('<QIIII', arguments)
    if count == 0 or count > min(capacity, 4096) or len(entries) != count * 8:
        raise ValueError('invalid GPU submission entry count')
    if flags & ~0x104:
        raise ValueError('unsupported GPU submission flags')
    decoded = []
    total_words = 0
    for low, high in struct.iter_unpack('<II', entries):
        entry = decode_entry(low, high)
        if entry is None:
            continue
        address, word_count, not_main, no_prefetch = entry
        total_words += word_count
        if total_words > 65536:
            raise ValueError('GPU submission word limit exceeded')
        raw = read_gpu(address, word_count * 4)
        if len(raw) != word_count * 4:
            raise ValueError('truncated GPU command buffer')
        words = struct.unpack('<' + 'I' * word_count, raw)
        methods = list(decode_packets(words))
        decoded.append((address, not_main, no_prefetch, methods))
    return (pointer, flags, fence_id, fence_value), decoded


MAX_WORDS = 1_048_576
MAX_WRITES = 1_048_576

def decode_entry(low, high):
    """Return (guest address, word count, not-main, no-prefetch), or None for NOP.

    This validates descriptor encoding only. A future executor must resolve and
    validate the entire range against its GPU address-space mappings.
    """
    if not 0 <= low <= 0xffffffff or not 0 <= high <= 0xffffffff:
        raise ValueError('invalid GPFIFO descriptor word')
    if low & 3 or high & 0x100:
        raise ValueError('unsupported conditional or privileged GPFIFO entry')
    count = (high >> 10) & 0x1fffff
    if count == 0:
        if low == 0 and high == 0:
            return None
        raise ValueError('unsupported GPFIFO control entry')
    if count > MAX_WORDS:
        raise ValueError('GPFIFO entry exceeds word limit')
    address = (low & 0xfffffffc) | ((high & 0xff) << 32)
    if address + count * 4 > 1 << 40:
        raise ValueError('GPFIFO address range overflows')
    return address, count, bool(high & 0x200), bool(high & 0x80000000)


def decode_packets(words):
    if len(words) > MAX_WORDS:
        raise ValueError('GPU command stream exceeds limit')
    index = 0
    writes = 0
    while index < len(words):
        header = words[index]
        index += 1
        if not 0 <= header <= 0xffffffff:
            raise ValueError('invalid GPU command word')
        if header == 0:
            continue  # NV906F NOP
        method = header & 0x1fff
        if method & 0x1000:
            raise ValueError('unsupported extended GPU method address')
        subchannel = (header >> 13) & 7
        argument = (header >> 16) & 0x1fff
        mode = (header >> 29) & 7
        if mode == 4:
            yield subchannel, method * 4, argument
            writes += 1
            if writes > MAX_WRITES:
                raise ValueError('GPU method count exceeds limit')
            continue
        if mode not in (1, 3, 5):
            raise ValueError(f'unsupported GPU packet mode {mode}')
        if argument > len(words) - index:
            raise ValueError('truncated GPU method packet')
        if writes + argument > MAX_WRITES:
            raise ValueError('GPU method count exceeds limit')
        packet = []
        for n in range(argument):
            word = words[index + n]
            if not 0 <= word <= 0xffffffff:
                raise ValueError('invalid GPU payload word')
            destination = method + (n if mode == 1 else min(n, 1) if mode == 5 else 0)
            if destination > 0xfff:
                raise ValueError('GPU method address overflow')
            packet.append((subchannel, destination * 4, word))
        index += argument
        writes += argument
        yield from packet
