#!/usr/bin/env python3
"""Experimental Switch homebrew runtime; compatibility is incomplete."""
import argparse
import ctypes
import copy
import datetime
import hashlib
import os
import secrets
import struct
import sys
import time
import zlib
from pathlib import Path

from audio_queue import AudioQueue
from buffer_queue import BufferQueue
from virtual_directory import Directory
from memory_files import MemoryFiles
from gpu_packets import gpu_characteristics, gpu_metadata_ioctl, inspect_submission, decode_entry
from gpu_memory import initialize_address_space, GpuChannel
from gpu_executor import GpuExecutionState, UnsupportedGpuMethod
from gpu_commit import commit_cpu_submission
from shared_fonts import build_font_image, MAX_FONT_SIZE
from save_store import SaveStore

try:
    from unicorn import arm64_const
    from unicorn import Uc, UcError, UC_ARCH_ARM64, UC_MODE_ARM, UC_HOOK_INTR, UC_HOOK_MEM_INVALID
    from unicorn.arm64_const import UC_ARM64_REG_PC, UC_ARM64_REG_SP, UC_ARM64_REG_X0, UC_ARM64_REG_X1, UC_ARM64_REG_X2, UC_ARM64_REG_X3, UC_ARM64_REG_X4, UC_ARM64_REG_X5, UC_ARM64_REG_TPIDRRO_EL0
except ImportError:
    sys.exit("Install the optional 'unicorn' Python package to use this diagnostic tracer.")

MAX_IMAGE = 64 * 1024 * 1024
BASE = 0x100000000
STACK = 0x200000000
STACK_SIZE = 2 * 1024 * 1024

# Optional native converter; the bounded Python path remains the fallback.
_native_convert = None
try:
    _switch_home = Path(os.environ.get('SOJU_SWITCH_HOME', Path.home() / '.local/share/soju/switch'))
    _native = ctypes.CDLL(str(_switch_home / 'libsoju-switch-frame.dylib'))
    _native_convert = _native.soju_switch_convert
    _native_convert.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_void_p, ctypes.c_size_t,
                                ctypes.c_uint32, ctypes.c_uint32, ctypes.c_uint32, ctypes.c_uint32, ctypes.c_uint32]
    _native_convert.restype = ctypes.c_int
except (OSError, AttributeError):
    _native_convert = None

def read_image(path):
    # Bound allocation before reading; appended NRO assets are not mapped.
    with path.open("rb") as f:
        header = f.read(0x80)
        if len(header) != 0x80 or header[0x10:0x14] != b"NRO0":
            raise ValueError("invalid or truncated NRO header")
        version, size = struct.unpack_from("<II", header, 0x14)
        bss = struct.unpack_from("<I", header, 0x38)[0]
        if version != 0 or size < 0x80 or size + bss > MAX_IMAGE:
            raise ValueError("unsupported version or image size")
        previous_end = 0
        for index in range(3):
            off, length = struct.unpack_from("<II", header, 0x20 + 8 * index)
            if off < previous_end or off > size or length > size - off:
                raise ValueError("invalid NRO segment layout")
            if index == 0 and (off != 0 or length < 0x80 or length % 4):
                raise ValueError("invalid text segment")
            previous_end = off + length
        f.seek(0)
        image = f.read(size)
        if len(image) != size:
            raise ValueError("truncated NRO image")
        return image, bss

def guest_range_has_permission(cpu, address, size, permission):
    if size == 0:
        return True
    end = address + size
    if address < 0 or end > (1 << 64):
        return False
    position = address
    for start, last, permissions in sorted(cpu.mem_regions()):
        if start <= position <= last and permissions & permission == permission:
            position = min(end, last + 1)
            if position == end:
                return True
    return False


def writable_guest_range(cpu, address, size):
    return guest_range_has_permission(cpu, address, size, 2)


def map_buffer(request, receive=False):
    h0, h1 = struct.unpack_from('<II', request)
    pos = 8
    if h1 & 0x80000000:
        special = struct.unpack_from('<I', request, pos)[0]
        pos += 4 + (8 if special & 1 else 0) + 4 * (((special >> 1) & 15) + ((special >> 5) & 15))
    pos += ((h0 >> 16) & 15) * 8
    sends, recvs = (h0 >> 20) & 15, (h0 >> 24) & 15
    if (recvs if receive else sends) != 1:
        raise ValueError('unsupported audio buffer descriptors')
    if receive:
        pos += sends * 12
    size_low, low, packed = struct.unpack_from('<III', request, pos)
    address = low | (((packed >> 28) & 15) << 32) | (((packed >> 2) & 0x3fffff) << 36)
    size = size_low | (((packed >> 24) & 15) << 32)
    return address, size


def cmif_offset(request, domain):
    h0, h1 = struct.unpack_from('<II', request)
    pos = 8
    if h1 & 0x80000000:
        special = struct.unpack_from('<I', request, pos)[0]
        pos += 4 + (8 if special & 1 else 0) + 4 * (((special >> 1) & 15) + ((special >> 5) & 15))
    pos += ((h0 >> 16) & 15) * 8
    pos += (((h0 >> 20) & 15) + ((h0 >> 24) & 15) + ((h0 >> 28) & 15)) * 12
    pos = (pos + 15) & ~15
    if domain:
        if pos + 16 > len(request):
            return -1
        if request[pos] == 2:  # domain close has no CMIF header
            return -1
        if request[pos] != 1:
            return -1
        pos += 16
    return pos if pos + 16 <= len(request) and request[pos:pos + 4] == b'SFCI' else -1


def binder_queue_slot(cpu, request):
    h0 = struct.unpack_from('<I', request)[0]
    if ((h0 >> 16) & 15) != 1:
        raise ValueError('unsupported binder input descriptor')
    packed, low = struct.unpack_from('<II', request, 8)
    address = low | (((packed >> 12) & 15) << 32) | (((packed >> 6) & 63) << 36)
    size = packed >> 16
    if not 32 <= size <= 1024:
        raise ValueError('invalid binder parcel size')
    raw = bytes(cpu.mem_read(address, size))
    payload_size, payload_offset = struct.unpack_from('<II', raw)
    if payload_offset != 16 or payload_size > len(raw) - payload_offset:
        raise ValueError('invalid binder parcel header')
    token, length = struct.unpack_from('<II', raw, payload_offset)
    if token != 0x100 or length > 128:
        raise ValueError('invalid binder interface token')
    end = payload_offset + 8 + (length + 1) * 2
    aligned = (end + 3) & ~3
    if aligned + 4 > payload_offset + payload_size:
        raise ValueError('truncated binder queue slot')
    expected = 'android.gui.IGraphicBufferProducer'.encode('utf-16le') + bytes(2)
    if raw[payload_offset + 8:end] != expected:
        raise ValueError('unsupported binder interface')
    return struct.unpack_from('<I', raw, aligned)[0]


def frame_pixels(cpu, buf, allocation):
    nv_id, width, height, pitch, plane_offset, log2_height, *formats = buf
    pixel_format = formats[0] if formats else 1
    if pixel_format not in (1, 5):
        raise ValueError('unsupported pixel format')
    allocation_size, allocation_address = allocation
    if (not 1 <= width <= 4096 or not 1 <= height <= 2160 or width * height > 4096 * 2160
            or pitch < width * 4 or pitch > 65536 or log2_height > 5):
        raise ValueError('unsupported display dimensions or layout')
    bh = 1 << log2_height
    block_rows = 8 * bh
    needed = pitch * ((height + block_rows - 1) // block_rows) * block_rows
    if pitch % 64 or needed > MAX_IMAGE or plane_offset > allocation_size or needed > allocation_size - plane_offset:
        raise ValueError('display buffer outside NV allocation')
    src = bytes(cpu.mem_read(allocation_address + plane_offset, needed))
    if _native_convert is not None:
        source = ctypes.create_string_buffer(src, len(src))
        target = ctypes.create_string_buffer(width * height * 4)
        if _native_convert(source, len(src), target, len(target), width, height, pitch, log2_height, pixel_format) != 0:
            raise ValueError('native display conversion failed')
        return target.raw
    rows = bytearray()
    for y in range(height):
        base = (y // block_rows) * pitch * block_rows + ((y // 8) % bh) * 512 + ((y % 8) // 2) * 64 + (y % 2) * 16
        for xb in range(0, width * 4, 16):
            offset = base + (xb // 64) * (512 * bh) + ((xb % 64) // 32) * 256 + ((xb % 32) // 16) * 32
            rows.extend(src[offset:offset + min(16, width * 4 - xb)])
    if pixel_format == 5:  # B8G8R8A8 to RGBA for SDL and PNG.
        rows[0::4], rows[2::4] = rows[2::4], rows[0::4]
    return bytes(rows)


def save_frame(cpu, buf, allocation, path):
    width, height = buf[1:3]
    pixels = frame_pixels(cpu, buf, allocation)
    rows = b''.join(bytes([0]) + pixels[y * width * 4:(y + 1) * width * 4] for y in range(height))
    def chunk(name, data):
        body = name + data
        return struct.pack('>I', len(data)) + body + struct.pack('>I', zlib.crc32(body) & 0xffffffff)
    png = (bytes.fromhex('89504e470d0a1a0a') + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0))
           + chunk(b'IDAT', zlib.compress(bytes(rows))) + chunk(b'IEND', b''))
    path.write_bytes(png)


def trace(path, max_steps, capture=None, moves=(), capture_first=None, play=False):
    image, bss = read_image(path)
    digest = hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b''):
            digest.update(chunk)
    game_hash = digest.hexdigest()
    program_id = 0x01ff000000000000 | int(game_hash[:12], 16)
    save_store = SaveStore(_switch_home, game_hash)
    input_path = path
    # A host-provided Latin font is a fallback, not Nintendo's system font.
    font_path = Path('/System/Library/Fonts/Supplemental/Arial.ttf')
    font_data, font_records = b'', ()
    try:
        standard_font = font_path.read_bytes() if font_path.stat().st_size <= 8 * 1024 * 1024 else b''
        unicode_path = _switch_home / 'host-unicode-bmp.ttf'
        unicode_font = unicode_path.read_bytes() if unicode_path.is_file() and unicode_path.stat().st_size <= MAX_FONT_SIZE else b''
        if standard_font:
            font_data, font_records = build_font_image(standard_font, unicode_font)
    except (OSError, ValueError):
        font_data, font_records = b'', ()
    window = None
    if play:
        from sdl_display import Window
        window = Window()
    uc = Uc(UC_ARCH_ARM64, UC_MODE_ARM)
    mapped = (len(image) + bss + 0xfff) & ~0xfff
    uc.mem_map(BASE, mapped)
    uc.mem_write(BASE, image)
    uc.mem_map(STACK, STACK_SIZE)
    # Kernel-provided per-thread TLS, zeroed in this single-thread diagnostic.
    uc.mem_map(0x300000000, 0x1000)
    uc.reg_write(UC_ARM64_REG_TPIDRRO_EL0, 0x300000000)
    uc.reg_write(UC_ARM64_REG_SP, STACK + STACK_SIZE - 0x1000)
    # Minimal homebrew ABI loader config: main thread, applet type, end marker.
    # This diagnostic models only one thread and does not yet implement applet services.
    config = 0x300001000
    uc.mem_map(config, 0x1000)
    uc.mem_map(0x300010000, 0x1000)  # guest user-exception context
    loader_return = 0x300020000
    uc.mem_map(loader_return, 0x1000)
    uc.mem_write(loader_return, struct.pack('<I', 0xd4000fc1))  # private SVC 0x7e
    uc.mem_protect(loader_return, 0x1000, 5)
    uc.reg_write(arm64_const.UC_ARM64_REG_X30, loader_return)
    argv = config + 0x200
    uc.mem_write(argv, b'sdmc:/switch/soju-current.nro\0')
    entries = [(1, 0, 0x101, 0), (5, 0, 0, argv), (7, 0, 2, 0), (0, 0, 0, 0)]
    uc.mem_write(config, b''.join(struct.pack('<IIQQ', *entry) for entry in entries))
    uc.reg_write(UC_ARM64_REG_X0, config)
    uc.reg_write(UC_ARM64_REG_X1, 0xffffffffffffffff)
    result = {"reason": "instruction budget or end address reached"}
    locked_pages = set()  # PermissionLocked cannot be cleared once set.
    uncached_pages = set()  # Guest-visible attribute; host emulation uses coherent memory.
    sessions = {0x102: 'sm:'}
    domain_objects = {}
    domain_directories = {}
    domain_files = {}
    domain_saves = {}
    domain_save_attrs = {}
    virtual_saves = {}
    committed_saves = {}
    saves_loaded = False
    save_readers = {}
    applet_objects = {}
    domain_owner = {}
    events = {}  # event handle -> signaled state
    vsync_events = {}
    event_aliases = {}
    applet_message_events = set()
    applet_messages = []
    exit_requested = False
    exit_deadline = None
    transfer_memory = {}
    shared_mappings = {}
    shared_handles = set()
    nv_transfer_memory = {}  # client owner -> copied transfer-memory range
    audio_outputs = {}
    stack_aliases = {}
    threads = {0x101: {'state': 'running', 'context': None, 'tls': 0x300000000, 'priority': 44}}
    current_thread = 0x101
    scheduling = False
    nv_fds = {}
    nv_fd_owners = {}
    gpu_channels = {}
    gpu_execution_states = {}
    gpu_spaces = {}
    nv_maps = {}
    nv_map_owners = {}
    nv_map_flags = {}
    display_queue = BufferQueue()
    vi_state = {'display': False, 'layer': False}
    directories = {'/', '/switch'}
    memory_files = MemoryFiles()  # volatile guest files; never mapped to host paths
    heap_size = 0
    hid_address = None
    hid_sample = 1
    frame_count = 0
    move_index = 0
    pressed = False
    buttons = {'left': 1 << 12, 'up': 1 << 13, 'right': 1 << 14, 'down': 1 << 15, 'b': 1 << 1}

    def request_guest_exit():
        nonlocal exit_requested, exit_deadline
        if not exit_requested:
            exit_requested = True
            applet_messages.append(4)
            exit_deadline = time.monotonic() + 30
            for handle in applet_message_events:
                if handle in events:
                    events[handle] = True
        return bool(applet_message_events)

    def pump_audio():
        now = time.monotonic_ns()
        for handle, deadline in list(vsync_events.items()):
            if now >= deadline:
                if handle in events:
                    events[handle] = True
                vsync_events[handle] = now + 16666667
        for audio in audio_outputs.values():
            if audio['queue'].pump(now):
                for handle in audio['events']:
                    if handle in events:
                        events[handle] = True

    def update_hid(cpu, mask):
        nonlocal hid_sample
        if hid_address is not None:
            hid_sample += 1
            cpu.mem_write(hid_address + 0x9a48, struct.pack('<QQQiiiiII', hid_sample, hid_sample, mask, 0, 0, 0, 0, 1, 0))

    def read_mutex(address):
        if address % 4 or not writable_guest_range(uc, address, 4):
            raise ValueError('invalid mutex address')
        return struct.unpack('<I', bytes(uc.mem_read(address, 4)))[0]

    def write_mutex(address, value):
        uc.mem_write(address, struct.pack('<I', value))

    def release_mutex(address, owner):
        value = read_mutex(address)
        if value & ~0x40000000 != owner:
            raise ValueError('mutex is not owned by the calling thread')
        waiters = [(h, t) for h, t in threads.items()
                   if t['state'] == 'mutex_wait' and t['mutex'] == address]
        if waiters:
            handle, target = waiters[0]
            write_mutex(address, handle | (0x40000000 if len(waiters) > 1 else 0))
            target['pending'] = (0, 0)
            target['state'] = 'runnable'
        else:
            write_mutex(address, 0)

    def on_interrupt(cpu, kind, unused):
        nonlocal heap_size, hid_address, frame_count, move_index, pressed, scheduling, committed_saves, saves_loaded, display_queue
        pc = cpu.reg_read(UC_ARM64_REG_PC)
        try:
            ins = struct.unpack("<I", bytes(cpu.mem_read(pc - 4, 4)))[0]
        except UcError:
            result["reason"] = f"interrupt {kind} at pc=0x{pc:x}"
            cpu.emu_stop()
            return
        if (ins & 0xffe0001f) == 0xd4000001:
            svc = (ins >> 5) & 0xffff
            if svc == 0x7e and pc - 4 == loader_return:
                code = cpu.reg_read(UC_ARM64_REG_X0) & 0xffffffff
                result['reason'] = ('guest exited normally' if code == 0 else f'guest returned error 0x{code:x}')
                cpu.emu_stop()
                return
            if svc == 7:  # ExitProcess
                result['reason'] = 'guest exited normally' if exit_requested else 'guest exited'
                cpu.emu_stop()
                return
            if svc == 0x25:  # GetThreadId for a known guest thread or current-thread pseudo-handle.
                handle = cpu.reg_read(UC_ARM64_REG_X1) & 0xffffffff
                if handle == 0xffff8000:
                    handle = current_thread
                if handle in threads:
                    cpu.reg_write(UC_ARM64_REG_X0, 0)
                    cpu.reg_write(UC_ARM64_REG_X1, handle)
                    return
                result['reason'] = f'unsupported GetThreadId handle=0x{handle:x}'
                cpu.emu_stop()
                return
            if svc == 0x26:  # Break is a guest diagnostic, not a missing service.
                reason = cpu.reg_read(UC_ARM64_REG_X0) & 0xffffffff
                address = cpu.reg_read(UC_ARM64_REG_X1)
                size = cpu.reg_read(UC_ARM64_REG_X2)
                if reason & 0x80000000:  # non-fatal debugger notification
                    return
                extra = ''
                if size == 4:
                    try:
                        code = struct.unpack('<I', bytes(cpu.mem_read(address, 4)))[0]
                        extra = f' result=0x{code:x}'
                    except UcError:
                        extra = ' (invalid result pointer)'
                result['reason'] = f'guest Break reason=0x{reason:x}{extra} at offset 0x{pc - BASE - 4:x}'
                cpu.emu_stop()
                return
            if svc == 1:  # SetHeapSize, bounded 256 MiB simulated heap.
                requested = cpu.reg_read(UC_ARM64_REG_X1)
                if requested <= 0x10000000 and requested % 0x200000 == 0:
                    try:
                        if requested > heap_size:
                            cpu.mem_map(0x400000000 + heap_size, requested - heap_size)
                        elif requested < heap_size:
                            if any(source + size > 0x400000000 + requested for _, source, size in stack_aliases):
                                result['reason'] = 'cannot shrink heap with active stack mappings'
                                cpu.emu_stop()
                                return
                            cpu.mem_unmap(0x400000000 + requested, heap_size - requested)
                        heap_size = requested
                        cpu.reg_write(UC_ARM64_REG_X0, 0)
                        cpu.reg_write(UC_ARM64_REG_X1, 0x400000000)
                        return
                    except UcError:
                        pass
                result["reason"] = f"unsupported SetHeapSize size=0x{requested:x}"
                cpu.emu_stop()
                return
            if svc == 2:  # SetMemoryPermission: bounded NRO mapping subset only.
                address = cpu.reg_read(UC_ARM64_REG_X0)
                size = cpu.reg_read(UC_ARM64_REG_X1)
                permissions = cpu.reg_read(UC_ARM64_REG_X2)
                if (address % 0x1000 == 0 and size > 0 and size % 0x1000 == 0
                        and BASE <= address < BASE + mapped
                        and size <= BASE + mapped - address
                        and permissions in (0, 1, 3)
                        and not any(page in locked_pages for page in range(address, address + size, 0x1000))):
                    try:
                        cpu.mem_protect(address, size, permissions)
                        cpu.reg_write(UC_ARM64_REG_X0, 0)
                        return
                    except UcError:
                        pass
                result["reason"] = f"unsupported SetMemoryPermission at offset 0x{pc - BASE - 4:x}"
                cpu.emu_stop()
                return
            if svc in (4, 5):  # MapMemory / UnmapMemory for a stack alias of heap memory.
                dest = cpu.reg_read(UC_ARM64_REG_X0)
                source = cpu.reg_read(UC_ARM64_REG_X1)
                size = cpu.reg_read(UC_ARM64_REG_X2)
                if dest % 0x1000 or source % 0x1000:
                    cpu.reg_write(UC_ARM64_REG_X0, 0xcc01)  # InvalidAddress
                    return
                if size == 0 or size % 0x1000:
                    cpu.reg_write(UC_ARM64_REG_X0, 0xca01)  # InvalidSize
                    return
                if dest + size > (1 << 64) or not (STACK <= dest and dest + size <= STACK + 0x10000000):
                    cpu.reg_write(UC_ARM64_REG_X0, 0xdc01)  # InvalidMemoryRange
                    return
                if source + size > (1 << 64):
                    cpu.reg_write(UC_ARM64_REG_X0, 0xd401)  # InvalidMemoryState
                    return
                if svc == 4 and (dest % 0x1000 == source % 0x1000 == size % 0x1000 == 0
                        and 0 < size <= 0x1000000 and STACK <= dest and dest + size <= STACK + 0x10000000
                        and 0x400000000 <= source and source + size <= 0x400000000 + heap_size
                        and guest_range_has_permission(cpu, source, size, 3)):
                    try:
                        data = bytes(cpu.mem_read(source, size))
                        cpu.mem_map(dest, size)
                        try:
                            cpu.mem_write(dest, data)
                            cpu.mem_protect(source, size, 0)
                        except UcError:
                            cpu.mem_unmap(dest, size)
                            raise
                        stack_aliases[(dest, source, size)] = True
                        cpu.reg_write(UC_ARM64_REG_X0, 0)
                        return
                    except UcError:
                        pass
                if svc == 5 and (dest, source, size) in stack_aliases:
                    try:
                        data = bytes(cpu.mem_read(dest, size))
                        cpu.mem_write(source, data)
                        cpu.mem_unmap(dest, size)
                        cpu.mem_protect(source, size, 3)
                        del stack_aliases[(dest, source, size)]
                        cpu.reg_write(UC_ARM64_REG_X0, 0)
                        return
                    except UcError:
                        pass
                result['reason'] = f'unsupported {"MapMemory" if svc == 4 else "UnmapMemory"} dest=0x{dest:x} source=0x{source:x} size=0x{size:x}'
                cpu.emu_stop()
                return
            if svc == 8:  # CreateThread; the new context remains suspended until StartThread.
                entry = cpu.reg_read(UC_ARM64_REG_X1)
                arg = cpu.reg_read(UC_ARM64_REG_X2)
                stack = cpu.reg_read(UC_ARM64_REG_X3)
                priority = cpu.reg_read(UC_ARM64_REG_X4) & 0xffffffff
                core = cpu.reg_read(UC_ARM64_REG_X5) & 0xffffffff
                if (len(threads) >= 16 or priority > 63 or core not in (0, 0xfffffffe, 0xffffffff)
                        or stack % 16 or stack < 8 or not guest_range_has_permission(cpu, stack - 8, 8, 2)
                        or not guest_range_has_permission(cpu, entry, 4, 4)):
                    result['reason'] = 'unsupported CreateThread parameters'
                    cpu.emu_stop()
                    return
                new_handle = next(h for h in range(0x500, 0x510) if h not in threads)
                tls = 0x310000000 + (new_handle - 0x500) * 0x1000
                try:
                    cpu.mem_map(tls, 0x1000)
                except UcError:
                    result['reason'] = 'cannot allocate thread TLS'
                    cpu.emu_stop()
                    return
                threads[new_handle] = {'state': 'suspended', 'context': None, 'entry': entry,
                                       'arg': arg, 'stack': stack, 'tls': tls, 'priority': priority}
                cpu.reg_write(UC_ARM64_REG_X0, 0)
                cpu.reg_write(UC_ARM64_REG_X1, new_handle)
                return
            if svc in (0x0c, 0x0d):  # Get/SetThreadPriority; scheduling remains experimental.
                handle = cpu.reg_read(UC_ARM64_REG_X1 if svc == 0x0c else UC_ARM64_REG_X0) & 0xffffffff
                if handle == 0xffff8000:
                    handle = current_thread
                priority = cpu.reg_read(UC_ARM64_REG_X1) & 0xffffffff
                if handle in threads and (svc == 0x0c or priority <= 63):
                    if svc == 0x0d:
                        threads[handle]['priority'] = priority
                    else:
                        cpu.reg_write(UC_ARM64_REG_X1, threads[handle]['priority'])
                    cpu.reg_write(UC_ARM64_REG_X0, 0)
                    return
                result['reason'] = 'unsupported thread priority'
                cpu.emu_stop()
                return
            if svc == 0x0f:  # Limit thread affinity to the one emulated core.
                handle = cpu.reg_read(UC_ARM64_REG_X0)
                preferred = cpu.reg_read(UC_ARM64_REG_X1) & 0xffffffff
                mask = cpu.reg_read(UC_ARM64_REG_X2)
                if handle in threads and preferred in (0, 0xfffffffe, 0xffffffff) and mask == 1:
                    threads[handle]['core_mask'] = mask
                    cpu.reg_write(UC_ARM64_REG_X0, 0)
                    return
                result['reason'] = 'unsupported thread affinity'
                cpu.emu_stop()
                return
            if svc == 9:  # StartThread
                handle = cpu.reg_read(UC_ARM64_REG_X0)
                if handle not in threads or threads[handle]['state'] != 'suspended':
                    result['reason'] = f'unsupported StartThread handle=0x{handle:x}'
                    cpu.emu_stop()
                    return
                threads[handle]['state'] = 'runnable'
                cpu.reg_write(UC_ARM64_REG_X0, 0)
                scheduling = True
                cpu.emu_stop()
                return
            if svc == 10:  # ExitThread
                threads[current_thread]['state'] = 'exited'
                scheduling = True
                cpu.emu_stop()
                return
            if svc == 11:  # SleepThread: yield or wait for a monotonic deadline.
                ns = cpu.reg_read(UC_ARM64_REG_X0)
                threads[current_thread]['last_sleep'] = (ns, cpu.reg_read(arm64_const.UC_ARM64_REG_X30))
                if ns < (1 << 63) and ns > 0:
                    threads[current_thread]['state'] = 'sleeping'
                    threads[current_thread]['deadline'] = time.monotonic_ns() + ns
                scheduling = True
                cpu.emu_stop()
                return
            if svc == 0x13:  # Map the HID shared memory object, initially no buttons.
                handle = cpu.reg_read(UC_ARM64_REG_X0)
                address = cpu.reg_read(UC_ARM64_REG_X1)
                size = cpu.reg_read(UC_ARM64_REG_X2)
                permissions = cpu.reg_read(UC_ARM64_REG_X3)
                if handle in shared_handles and handle == 0x300 and size == 0x40000 and address % 0x1000 == 0 and permissions == 1:
                    try:
                        cpu.mem_map(address, size)
                        # Player one: full-key controller, one connected sample.
                        cpu.mem_write(address + 0x9a00, struct.pack('<I', 1))
                        cpu.mem_write(address + 0x9a28, struct.pack('<QQQQ', 0, 17, 0, 1))
                        cpu.mem_write(address + 0x9a48, struct.pack('<QQQiiiiII', 1, 1, 0, 0, 0, 0, 0, 1, 0))
                        hid_address = address
                        cpu.mem_protect(address, size, 1)
                        shared_mappings[(handle, address)] = size
                        cpu.reg_write(UC_ARM64_REG_X0, 0)
                        return
                    except UcError:
                        pass
                if handle in shared_handles and handle == 0x301 and size == 0x1000 and address % 0x1000 == 0 and permissions == 1:
                    try:
                        cpu.mem_map(address, size)
                        # Approximate host wall clock; steady-clock sync is not yet implemented.
                        context = struct.pack('<qq16s', int(time.time()), 0, bytes(16))
                        for offset in (0x38, 0x80):
                            cpu.mem_write(address + offset + 8, context + context)
                        cpu.mem_protect(address, size, 1)
                        shared_mappings[(handle, address)] = size
                        cpu.reg_write(UC_ARM64_REG_X0, 0)
                        return
                    except UcError:
                        pass
                if handle in shared_handles and handle == 0x302 and size == 0x1100000 and address % 0x1000 == 0 and permissions == 1 and font_data:
                    try:
                        cpu.mem_map(address, size)
                        cpu.mem_write(address, font_data)
                        cpu.mem_protect(address, size, 1)
                        shared_mappings[(handle, address)] = size
                        cpu.reg_write(UC_ARM64_REG_X0, 0)
                        return
                    except UcError:
                        pass
                result['reason'] = f'unsupported MapSharedMemory handle=0x{handle:x}, size=0x{size:x}'
                cpu.emu_stop()
                return
            if svc == 0x14:  # UnmapSharedMemory requires the original handle and range.
                handle = cpu.reg_read(UC_ARM64_REG_X0)
                address = cpu.reg_read(UC_ARM64_REG_X1)
                size = cpu.reg_read(UC_ARM64_REG_X2)
                if shared_mappings.get((handle, address)) != size:
                    result['reason'] = f'unsupported UnmapSharedMemory handle=0x{handle:x}, size=0x{size:x}'
                    cpu.emu_stop()
                    return
                cpu.mem_unmap(address, size)
                del shared_mappings[(handle, address)]
                if handle == 0x300 and hid_address == address:
                    hid_address = None
                cpu.reg_write(UC_ARM64_REG_X0, 0)
                return
            if svc == 0x17:  # ResetSignal for readable event handles.
                handle = cpu.reg_read(UC_ARM64_REG_X0)
                if handle in events:
                    for alias in event_aliases.get(handle, [handle]):
                        if alias in events:
                            events[alias] = False
                    cpu.reg_write(UC_ARM64_REG_X0, 0)
                    return
                result['reason'] = f'unsupported ResetSignal handle=0x{handle:x}'
                cpu.emu_stop()
                return
            if svc == 0x18:  # WaitSynchronization for local diagnostic events.
                pump_audio()
                pointer = cpu.reg_read(UC_ARM64_REG_X1)
                count = cpu.reg_read(UC_ARM64_REG_X2)
                timeout = cpu.reg_read(UC_ARM64_REG_X3)
                if 0 < count <= 64:
                    try:
                        handles = struct.unpack('<' + 'I' * count, bytes(cpu.mem_read(pointer, count * 4)))
                        if all(h in events or h in threads for h in handles):
                            ready = next((index for index, h in enumerate(handles)
                                          if (h in events and events[h]) or (h in threads and threads[h]['state'] == 'exited')), None)
                            if ready is not None:
                                cpu.reg_write(UC_ARM64_REG_X0, 0)
                                cpu.reg_write(UC_ARM64_REG_X1, ready)
                                return
                            if timeout == 0:
                                cpu.reg_write(UC_ARM64_REG_X0, 0xea01)
                                return
                            threads[current_thread]['state'] = 'waiting'
                            threads[current_thread]['wait_handles'] = handles
                            threads[current_thread]['deadline'] = (time.monotonic_ns() + timeout
                                                                      if timeout < (1 << 63) else None)
                            scheduling = True
                            cpu.emu_stop()
                            return
                    except UcError:
                        pass
                result['reason'] = 'unsupported blocking wait or handle'
                cpu.emu_stop()
                return
            if svc == 0x15:  # CreateTransferMemory from a bounded heap range.
                address = cpu.reg_read(UC_ARM64_REG_X1)
                size = cpu.reg_read(UC_ARM64_REG_X2)
                permissions = cpu.reg_read(UC_ARM64_REG_X3)
                if (address % 0x1000 == 0 and size > 0 and size % 0x1000 == 0
                        and 0x400000000 <= address < 0x400000000 + heap_size
                        and size <= 0x400000000 + heap_size - address and permissions in (0, 1, 3)
                        and guest_range_has_permission(cpu, address, size, 3)
                        and not any(address < a + n and a < address + size
                                    for a, n in list(transfer_memory.values()) + list(nv_transfer_memory.values()))):
                    try:
                        new_handle = next((h for h in range(0x400, 0x500) if h not in transfer_memory), None)
                        if new_handle is None:
                            raise ValueError('transfer-memory handle limit reached')
                        cpu.mem_protect(address, size, permissions)
                        transfer_memory[new_handle] = (address, size)
                        cpu.reg_write(UC_ARM64_REG_X0, 0)
                        cpu.reg_write(UC_ARM64_REG_X1, new_handle)
                        return
                    except (UcError, ValueError):
                        pass
                result['reason'] = f'unsupported CreateTransferMemory address=0x{address:x} size=0x{size:x}'
                cpu.emu_stop()
                return
            if svc == 0x16:  # CloseHandle for emulated sessions.
                handle = cpu.reg_read(UC_ARM64_REG_X0)
                if handle in sessions:
                    closed_audio = audio_outputs.pop(handle, None)
                    if closed_audio and closed_audio['device']:
                        closed_audio['device'].close()
                    owner = domain_owner.pop(handle, handle)
                    del sessions[handle]
                    if owner not in sessions and owner not in domain_owner.values():
                        for fd in [fd for fd, fd_owner in nv_fd_owners.items() if fd_owner == owner]:
                            nv_fds.pop(fd, None)
                            nv_fd_owners.pop(fd, None)
                            gpu_channels.pop(fd, None)
                            gpu_execution_states.pop(fd, None)
                            gpu_spaces.pop(fd, None)
                        for nv_id in [nv_id for nv_id, nv_owner in nv_map_owners.items() if nv_owner == owner]:
                            nv_maps.pop(nv_id, None)
                            nv_map_flags.pop(nv_id, None)
                            nv_map_owners.pop(nv_id, None)
                        if owner in nv_transfer_memory:
                            address, size = nv_transfer_memory.pop(owner)
                            cpu.mem_protect(address, size, 3)
                        for table in (domain_objects, domain_directories, domain_files, domain_saves, domain_save_attrs, save_readers, applet_objects):
                            for key in [key for key in table if key[0] == owner]:
                                del table[key]
                    cpu.reg_write(UC_ARM64_REG_X0, 0)
                    return
                if handle in threads and handle != 0x101 and threads[handle]['state'] == 'exited':
                    cpu.mem_unmap(threads[handle]['tls'], 0x1000)
                    del threads[handle]
                    cpu.reg_write(UC_ARM64_REG_X0, 0)
                    return
                if handle in events:
                    vsync_events.pop(handle, None)
                    applet_message_events.discard(handle)
                    aliases = event_aliases.pop(handle, None)
                    if aliases is not None:
                        aliases.remove(handle)
                    del events[handle]
                    cpu.reg_write(UC_ARM64_REG_X0, 0)
                    return
                if handle in shared_handles:
                    shared_handles.remove(handle)
                    cpu.reg_write(UC_ARM64_REG_X0, 0)
                    return
                if handle in transfer_memory:
                    # A copied NV reference can outlive the guest handle.
                    region = transfer_memory.pop(handle)
                    if region not in nv_transfer_memory.values():
                        cpu.mem_protect(*region, 3)
                    cpu.reg_write(UC_ARM64_REG_X0, 0)
                    return
                result['reason'] = f'unsupported CloseHandle 0x{handle:x}'
                cpu.emu_stop()
                return
            if svc == 0x1a:  # ArbitrateLock
                owner = cpu.reg_read(UC_ARM64_REG_X0) & 0xffffffff
                address = cpu.reg_read(UC_ARM64_REG_X1)
                tag = cpu.reg_read(UC_ARM64_REG_X2) & 0xffffffff
                try:
                    value = read_mutex(address)
                    if tag != current_thread:
                        raise ValueError('incorrect mutex owner tag')
                    if value == owner | 0x40000000:
                        threads[current_thread].update(state='mutex_wait', mutex=address)
                        scheduling = True
                        cpu.emu_stop()
                    else:
                        cpu.reg_write(UC_ARM64_REG_X0, 0)
                except (UcError, ValueError) as exc:
                    result['reason'] = f'mutex lock: {exc}'
                    cpu.emu_stop()
                return
            if svc == 0x1b:  # ArbitrateUnlock
                try:
                    release_mutex(cpu.reg_read(UC_ARM64_REG_X0), current_thread)
                    cpu.reg_write(UC_ARM64_REG_X0, 0)
                except (UcError, ValueError) as exc:
                    result['reason'] = f'mutex unlock: {exc}'
                    cpu.emu_stop()
                return
            if svc == 0x1c:  # Atomic condition wait releases the mutex.
                mutex = cpu.reg_read(UC_ARM64_REG_X0)
                key = cpu.reg_read(UC_ARM64_REG_X1)
                tag = cpu.reg_read(UC_ARM64_REG_X2) & 0xffffffff
                timeout = cpu.reg_read(UC_ARM64_REG_X3)
                try:
                    if tag != current_thread or key % 4 or not writable_guest_range(cpu, key, 4):
                        raise ValueError('invalid condition wait')
                    release_mutex(mutex, current_thread)
                    threads[current_thread].update(state='cond_wait', key=key, mutex=mutex,
                        deadline=time.monotonic_ns() + timeout if timeout < (1 << 63) else None)
                    scheduling = True
                    cpu.emu_stop()
                except (UcError, ValueError) as exc:
                    result['reason'] = f'condition wait: {exc}'
                    cpu.emu_stop()
                return
            if svc == 0x1d:  # Signal condition waiters, reacquiring their mutex before return.
                key = cpu.reg_read(UC_ARM64_REG_X0)
                count = cpu.reg_read(UC_ARM64_REG_X1) & 0xffffffff
                try:
                    if key % 4 or not writable_guest_range(cpu, key, 4):
                        raise ValueError('invalid condition variable')
                    waiters = [(h, t) for h, t in threads.items() if t['state'] == 'cond_wait' and t['key'] == key]
                    if count != 0xffffffff:
                        waiters = waiters[:count]
                    for handle, target in waiters:
                        address = target['mutex']
                        value = read_mutex(address)
                        if value == 0:
                            write_mutex(address, handle)
                            target['pending'] = (0, 0)
                            target['state'] = 'runnable'
                        else:
                            write_mutex(address, value | 0x40000000)
                            target['state'] = 'mutex_wait'
                except (UcError, ValueError) as exc:
                    result['reason'] = f'condition signal: {exc}'
                    cpu.emu_stop()
                return
            if svc == 0x1e:  # GetSystemTick, using the Switch 19.2 MHz counter frequency.
                cpu.reg_write(UC_ARM64_REG_X0, time.monotonic_ns() * 192 // 10000)
                return
            if svc == 0x1f:  # ConnectToNamedPort, service manager only.
                name_pointer = cpu.reg_read(UC_ARM64_REG_X1)
                try:
                    name = bytes(cpu.mem_read(name_pointer, 4))
                except UcError:
                    name = b''
                if name == b'sm:' + bytes([0]):
                    cpu.reg_write(UC_ARM64_REG_X0, 0)
                    cpu.reg_write(UC_ARM64_REG_X1, 0x102)
                    return
                result["reason"] = f"unsupported named port {name!r} at 0x{name_pointer:x}"
                cpu.emu_stop()
                return
            if svc == 0x21:  # Minimal SM registration and CMIF pointer buffer query.
                handle = cpu.reg_read(UC_ARM64_REG_X0)
                tls_address = cpu.reg_read(UC_ARM64_REG_TPIDRRO_EL0)
                request = bytes(cpu.mem_read(tls_address, 0x100))
                msg_type = struct.unpack_from('<I', request)[0] & 0xffff
                if msg_type == 2 and handle in sessions:  # HIPC close
                    cpu.reg_write(UC_ARM64_REG_X0, 0)
                    return
                domain_session = msg_type == 4 and (domain_owner.get(handle, handle), 1) in domain_objects
                cmif = cmif_offset(request, domain_session)
                if cmif < 0 and msg_type == 4 and handle in sessions:
                    h0, h1 = struct.unpack_from('<II', request)
                    if not (h1 & 0x80000000):
                        pos = 8 + ((h0 >> 16) & 15) * 8 + (((h0 >> 20) & 15) + ((h0 >> 24) & 15) + ((h0 >> 28) & 15)) * 12
                        pos = (pos + 15) & ~15
                        if pos + 16 <= len(request):
                            typ, count, data_size, object_id = struct.unpack_from('<BBHI', request, pos)
                            owner = domain_owner.get(handle, handle)
                            if typ == 2 and count == 0 and data_size == 0 and (owner, object_id) in domain_objects:
                                del domain_objects[(owner, object_id)]
                                domain_directories.pop((owner, object_id), None)
                                domain_files.pop((owner, object_id), None)
                                domain_saves.pop((owner, object_id), None)
                                domain_save_attrs.pop((owner, object_id), None)
                                save_readers.pop((owner, object_id), None)
                                applet_objects.pop((owner, object_id), None)
                                cpu.mem_write(tls_address, bytes(0x100))
                                cpu.reg_write(UC_ARM64_REG_X0, 0)
                                return
                if cmif < 0 or cmif + 16 > len(request):
                    result['reason'] = f'invalid IPC header for session=0x{handle:x}'
                    cpu.emu_stop()
                    return
                command = struct.unpack_from('<I', request, cmif + 8)[0]
                is_query = handle in sessions and msg_type == 5 and command == 3
                is_register = handle == 0x102 and msg_type == 4 and command == 0
                requested = request[cmif + 16:cmif + 24].split(bytes([0]))[0] if cmif >= 0 else b''
                if handle == 0x102 and msg_type == 4 and command == 1 and not requested:
                    response = bytearray(0x100)
                    struct.pack_into('<II', response, 0, 0, 8)
                    struct.pack_into('<4sIII', response, 0x10, b'SFCO', 0, 0xc15, 0)
                    cpu.mem_write(tls_address, bytes(response))
                    cpu.reg_write(UC_ARM64_REG_X0, 0)
                    return
                is_open = (handle == 0x102 and msg_type == 4 and command == 1
                           and requested in (b'set:sys', b'set', b'appletAE', b'hid', b'time:u', b'time:s', b'fsp-srv', b'vi:m', b'nvdrv:a', b'bsd:s', b'bsd:u', b'pl:u', b'ns:am2', b'acc:u1', b'apm:p', b'audout:u', b'psm'))
                if sessions.get(handle) == 'audout:u' and msg_type == 4 and command == 1:
                    try:
                        name_address, name_size = map_buffer(request, True)
                        input_address, input_size = map_buffer(request)
                        if input_size < 0x100 or not guest_range_has_permission(cpu, input_address, 0x100, 1):
                            raise ValueError('invalid audio device name')
                        input_name = bytes(cpu.mem_read(input_address, 0x100)).split(bytes(1), 1)[0]
                        if input_name not in (b'', b'DeviceOut'):
                            raise ValueError('unsupported audio device name')
                        rate, channels = struct.unpack_from('<II', request, cmif + 16)
                        if rate != 48000 or channels not in (2, 0x20000) or name_size < 0x100 or not writable_guest_range(cpu, name_address, 0x100):
                            raise ValueError('unsupported audio device format')
                        cpu.mem_write(name_address, b'DeviceOut' + bytes(0x100 - 9))
                        new_handle = next(h for h in range(0x103, 0x200) if h not in sessions and h not in domain_owner.values())
                        sessions[new_handle] = 'audio:output'
                        device = None
                        if window:
                            try:
                                from audio_output import AudioDevice
                                device = AudioDevice(window.sdl)
                            except RuntimeError as exc:
                                print(f'soju-switch: audio unavailable: {exc}', file=sys.stderr)
                        audio_outputs[new_handle] = {'queue': AudioQueue(), 'events': [], 'device': device}
                        response = bytearray(0x100)
                        struct.pack_into('<IIII', response, 0, 0, 0x8000000c, 0x20, new_handle)
                        struct.pack_into('<4sIII', response, 0x10, b'SFCO', 0, 0, 0)
                        struct.pack_into('<IIII', response, 0x20, 48000, 2, 2, 1)
                        cpu.mem_write(tls_address, bytes(response))
                        cpu.reg_write(UC_ARM64_REG_X0, 0)
                    except (ValueError, UcError, struct.error) as exc:
                        result['reason'] = f'audio open: {exc}'
                        cpu.emu_stop()
                    return
                if sessions.get(handle) == 'audio:output' and msg_type == 4 and command in (0, 1, 2, 3, 4, 5, 6):
                    try:
                        audio = audio_outputs[handle]
                        pump_audio()
                        response = bytearray(0x100)
                        struct.pack_into('<II', response, 0, 0, (0x80000000 if command == 4 else 0) | 12)
                        struct.pack_into('<4sIII', response, 0x10, b'SFCO', 0, 0, 0)
                        if command == 0:
                            struct.pack_into('<I', response, 0x20, 0 if audio['queue'].running else 1)
                        elif command in (1, 2):
                            audio['queue'].set_running(command == 1, time.monotonic_ns())
                            if audio['device']:
                                audio['device'].pause(not audio['queue'].running)
                        elif command == 3:
                            if len(audio['queue'].pending) + len(audio['queue'].released) >= 64:
                                raise ValueError('audio queue is full')
                            tag = struct.unpack_from('<Q', request, cmif + 16)[0]
                            address, size = map_buffer(request)
                            if size < 40 or not guest_range_has_permission(cpu, address, 40, 1):
                                raise ValueError('invalid audio buffer')
                            next_ptr, pcm, capacity, length, offset = struct.unpack('<QQQQQ', bytes(cpu.mem_read(address, 40)))
                            # Historical devkitPro SDL reported frame units as buffer_size.
                            legacy_sdl_units = pcm % 0x1000 == 0 and capacity > 0 and length == capacity * 8
                            if next_ptr or offset or (length > capacity and not legacy_sdl_units) or length % 4 or length > 8 * 1024 * 1024 or not guest_range_has_permission(cpu, pcm, length, 1):
                                raise ValueError(f'unsupported audio data layout next=0x{next_ptr:x} pcm=0x{pcm:x} capacity={capacity} length={length} offset={offset}')
                            if audio['device'] and length:
                                audio['device'].queue(bytes(cpu.mem_read(pcm, length)))
                            now = time.monotonic_ns()
                            audio['queue'].append(tag, length * 1000000000 // (48000 * 4), now)
                        elif command == 4:
                            event_handle = next(h for h in range(0x200, 0x300) if h not in events)
                            events[event_handle] = any(events.get(h, False) for h in audio['events'])
                            audio['events'].append(event_handle)
                            event_aliases[event_handle] = audio['events']
                            struct.pack_into('<II', response, 8, 2, event_handle)
                        elif command == 5:
                            address, size = map_buffer(request, True)
                            count = min(len(audio['queue'].released), size // 8)
                            if size > 8 * 1024 * 1024 or not writable_guest_range(cpu, address, count * 8):
                                raise ValueError('invalid audio release buffer')
                            if count:
                                cpu.mem_write(address, struct.pack('<' + 'Q' * count, *audio['queue'].pop_released(count)))
                            struct.pack_into('<I', response, 0x20, count)
                        else:
                            tag = struct.unpack_from('<Q', request, cmif + 16)[0]
                            struct.pack_into('<B', response, 0x20, int(audio['queue'].contains(tag)))
                        cpu.mem_write(tls_address, bytes(response))
                        cpu.reg_write(UC_ARM64_REG_X0, 0)
                    except (KeyError, ValueError, RuntimeError, UcError, struct.error) as exc:
                        result['reason'] = f'audio output: {exc}'
                        cpu.emu_stop()
                    return
                is_apm_session = sessions.get(handle) == 'apm:p' and msg_type == 4 and command == 0
                is_apm_mode = sessions.get(handle) == 'apm:p' and msg_type == 4 and command == 1
                is_account_registration = sessions.get(handle) == 'acc:u1' and msg_type == 4 and command == 50
                is_font = sessions.get(handle) == 'pl:u' and msg_type == 4 and command in (0, 1, 2, 3, 4, 5) and bool(font_data)
                font_type = struct.unpack_from('<I', request, cmif + 16)[0] if is_font and command in (0, 1, 2, 3) else 0
                if is_font and command in (0, 1, 2, 3) and font_type >= len(font_records):
                    result['reason'] = f'requested shared font type {font_type} is not available'
                    cpu.emu_stop()
                    return
                if is_font and command == 5:
                    try:
                        h0, h1 = struct.unpack_from('<II', request)
                        if ((h0 >> 24) & 15) != 3 or ((h0 >> 28) & 15):
                            raise ValueError('unsupported shared font list descriptors')
                        pos = 8
                        if h1 & 0x80000000:
                            special = struct.unpack_from('<I', request, pos)[0]
                            pos += 4 + (8 if special & 1 else 0) + 4 * (((special >> 1) & 15) + ((special >> 5) & 15))
                        pos += ((h0 >> 16) & 15) * 8 + ((h0 >> 20) & 15) * 12
                        outputs = []
                        for i in range(3):
                            size_low, low, packed = struct.unpack_from('<III', request, pos + i * 12)
                            out = low | (((packed >> 28) & 15) << 32) | (((packed >> 2) & 0x3fffff) << 36)
                            size = size_low | (((packed >> 24) & 15) << 32)
                            if size < 24 or not writable_guest_range(cpu, out, 24):
                                raise ValueError('invalid shared font list output buffer')
                            if (out < tls_address + 0x100 and tls_address < out + 24
                                    or any(out < previous + 24 and previous < out + 24 for previous in outputs)):
                                raise ValueError('overlapping shared font list outputs')
                            outputs.append(out)
                        arrays = (range(6), (r[0] for r in font_records), (r[1] for r in font_records))
                        for out, values in zip(outputs, arrays):
                            cpu.mem_write(out, struct.pack('<6I', *values))
                    except (ValueError, UcError, struct.error) as exc:
                        result['reason'] = f'shared font list: {exc}'
                        cpu.emu_stop()
                        return
                is_bsd_init = sessions.get(handle) in ('bsd:s', 'bsd:u') and msg_type == 4 and command in (0, 1)
                is_firmware = sessions.get(handle) == 'set:sys' and msg_type == 4 and command in (3, 4)
                is_nv_init = sessions.get(handle) == 'nvdrv:a' and msg_type == 4 and command in (3, 8)
                if is_nv_init and command == 3:
                    h1 = struct.unpack_from('<I', request, 4)[0]
                    special = struct.unpack_from('<I', request, 8)[0] if h1 & 0x80000000 else 0
                    copy_count = (special >> 1) & 15
                    handle_pos = 12 + (8 if special & 1 else 0)
                    copied = struct.unpack_from('<' + 'I' * copy_count, request, handle_pos)
                    transfer_handles = [h for h in copied if h in transfer_memory]
                    group = domain_owner.get(handle, handle)
                    move_count = (special >> 5) & 15
                    transfer_size = struct.unpack_from('<I', request, cmif + 16)[0]
                    if (copy_count != 2 or move_count != 0 or copied[0] != 0xffff8001
                            or len(transfer_handles) != 1 or copied[1] != transfer_handles[0]
                            or group in nv_transfer_memory
                            or transfer_memory[transfer_handles[0]] in nv_transfer_memory.values()
                            or transfer_size != transfer_memory[transfer_handles[0]][1]):
                        result['reason'] = 'unsupported NV initialize transfer-memory descriptor'
                        cpu.emu_stop()
                        return
                    nv_transfer_memory[group] = transfer_memory[transfer_handles[0]]
                is_nv_open = sessions.get(handle) == 'nvdrv:a' and msg_type == 4 and command == 0
                is_nv_ioctl = sessions.get(handle) == 'nvdrv:a' and msg_type == 4 and command == 1
                is_nv_close = sessions.get(handle) == 'nvdrv:a' and msg_type == 4 and command == 2
                is_nv_event = sessions.get(handle) == 'nvdrv:a' and msg_type == 4 and command == 4
                if is_nv_event:
                    event_fd, event_id = struct.unpack_from('<II', request, cmif + 16)
                    if (event_fd not in gpu_channels or event_id != 3
                            or gpu_channels[event_fd].owner != domain_owner.get(handle, handle)):
                        result['reason'] = 'unsupported NV channel event'
                        cpu.emu_stop()
                        return
                if is_nv_close:
                    close_fd = struct.unpack_from('<I', request, cmif + 16)[0]
                    if close_fd not in nv_fds:
                        result['reason'] = f'unsupported NV close fd={close_fd}'
                        cpu.emu_stop()
                        return
                    if nv_fd_owners[close_fd] != domain_owner.get(handle, handle):
                        result['reason'] = 'NV close belongs to a different client'
                        cpu.emu_stop()
                        return
                    del nv_fds[close_fd]
                    del nv_fd_owners[close_fd]
                    gpu_channels.pop(close_fd, None)
                    gpu_execution_states.pop(close_fd, None)
                    gpu_spaces.pop(close_fd, None)
                nv_fd = 0
                if is_nv_ioctl:
                    h0, h1 = struct.unpack_from('<II', request)
                    if ((h0 >> 16) & 15, (h0 >> 20) & 15, (h0 >> 24) & 15) != (1, 1, 1):
                        result['reason'] = 'unsupported NV ioctl descriptors'
                        cpu.emu_stop()
                        return
                    def nv_desc(offset):
                        size, low, desc = struct.unpack_from('<III', request, offset)
                        return (low | (((desc >> 28) & 15) << 32) | (((desc >> 2) & 0x3fffff) << 36),
                                size | (((desc >> 24) & 15) << 32))
                    source, in_size = nv_desc(16)
                    target, out_size = nv_desc(28)
                    fd, ioctl = struct.unpack_from('<II', request, cmif + 16)
                    if nv_fd_owners.get(fd) != domain_owner.get(handle, handle):
                        result['reason'] = 'invalid NV file descriptor or client owner'
                        cpu.emu_stop()
                        return
                    if ((in_size and not guest_range_has_permission(cpu, source, in_size, 1))
                            or (out_size and not writable_guest_range(cpu, target, out_size))):
                        result['reason'] = 'invalid NV ioctl buffer permissions'
                        cpu.emu_stop()
                        return
                    supported_nvmap = nv_fds.get(fd) == b'/dev/nvmap' and in_size <= 64
                    supported_gpu_info = nv_fds.get(fd) == b'/dev/nvhost-ctrl-gpu' and ioctl == 0xc0b04705 and in_size == 176
                    supported_gpu_metadata = (nv_fds.get(fd) == b'/dev/nvhost-ctrl-gpu'
                                              and ioctl in (0x80044701, 0x80284702))
                    supported_as_init = (nv_fds.get(fd) == b'/dev/nvhost-as-gpu'
                                         and ioctl == 0x40284109 and in_size == 40 and out_size == 0)
                    supported_as_bind = (nv_fds.get(fd) == b'/dev/nvhost-as-gpu'
                                         and ioctl == 0x40044101 and in_size == 4 and out_size == 0)
                    supported_set_nvmap = (nv_fds.get(fd) == b'/dev/nvhost-gpu'
                                           and ioctl == 0x40044801 and in_size == 4 and out_size == 0)
                    supported_gpfifo_alloc = (nv_fds.get(fd) == b'/dev/nvhost-gpu'
                                              and ioctl == 0xc020481a and in_size == 32 and out_size == 32)
                    supported_gpu_object = (nv_fds.get(fd) == b'/dev/nvhost-gpu'
                                            and ioctl == 0xc0104809 and in_size == 16 and out_size == 16)
                    supported_notifier = (nv_fds.get(fd) == b'/dev/nvhost-gpu'
                                          and ioctl == 0xc018480c and in_size == 24 and out_size == 24)
                    supported_gpu_priority = (nv_fds.get(fd) == b'/dev/nvhost-gpu'
                                              and ioctl == 0x4004480d and in_size == 4 and out_size == 0)
                    supported_as_map = (nv_fds.get(fd) == b'/dev/nvhost-as-gpu'
                                        and ioctl == 0xc0284106 and in_size == 40 and out_size == 40)
                    supported_zcull_bind = (nv_fds.get(fd) == b'/dev/nvhost-gpu'
                                            and ioctl == 0xc010480b and in_size == 16 and out_size == 16)
                    supported_as_alloc = (nv_fds.get(fd) == b'/dev/nvhost-as-gpu'
                                          and ioctl == 0xc0184102 and in_size == 24 and out_size == 24)
                    if not (supported_nvmap or supported_gpu_info or supported_gpu_metadata or supported_as_init or supported_as_alloc or supported_as_bind or supported_set_nvmap or supported_gpfifo_alloc or supported_gpu_object or supported_notifier or supported_gpu_priority or supported_as_map or supported_zcull_bind) or (not (supported_as_init or supported_as_bind or supported_set_nvmap or supported_gpu_priority) and out_size < in_size):
                        result['reason'] = f'unsupported NV ioctl fd={fd} ioctl=0x{ioctl:x} input_size={in_size} output_size={out_size}'
                        if os.environ.get('SOJU_SWITCH_TRACE_GPU') and in_size <= 64:
                            try:
                                result['reason'] += f' input={bytes(cpu.mem_read(source, in_size)).hex()}'
                            except UcError:
                                pass
                        cpu.emu_stop()
                        return
                    try:
                        data = bytearray(cpu.mem_read(source, in_size)) if in_size else bytearray()
                        if supported_as_init:
                            if fd in gpu_spaces or not guest_range_has_permission(cpu, source, in_size, 1):
                                raise ValueError('invalid GPU address-space initialization buffer or duplicate initialization')
                            gpu_spaces[fd] = (domain_owner.get(handle, handle), initialize_address_space(data))
                            data = bytearray()
                        elif supported_zcull_bind:
                            group = domain_owner.get(handle, handle)
                            if (fd not in gpu_channels or gpu_channels[fd].owner != group
                                    or not guest_range_has_permission(cpu, source, in_size, 1)
                                    or not writable_guest_range(cpu, target, out_size)):
                                raise ValueError('invalid GPU ZCull binding buffer')
                            gpu_channels[fd].bind_zcull(*struct.unpack('<QII', data))
                        elif supported_gpu_priority:
                            group = domain_owner.get(handle, handle)
                            if (fd not in gpu_channels or gpu_channels[fd].owner != group
                                    or not guest_range_has_permission(cpu, source, in_size, 1)):
                                raise ValueError('invalid GPU channel priority buffer')
                            gpu_channels[fd].set_priority(struct.unpack('<I', data)[0])
                            data = bytearray()
                        elif supported_notifier:
                            group = domain_owner.get(handle, handle)
                            if (fd not in gpu_channels or gpu_channels[fd].owner != group
                                    or not guest_range_has_permission(cpu, source, in_size, 1)
                                    or not writable_guest_range(cpu, target, out_size)):
                                raise ValueError('invalid GPU error notifier buffer')
                            gpu_channels[fd].set_error_notifier(*struct.unpack('<QQII', data))
                        elif supported_gpu_object:
                            group = domain_owner.get(handle, handle)
                            if (fd not in gpu_channels or gpu_channels[fd].owner != group
                                    or not guest_range_has_permission(cpu, source, in_size, 1)
                                    or not writable_guest_range(cpu, target, out_size)):
                                raise ValueError('invalid GPU object allocation buffer')
                            class_id, flags = struct.unpack_from('<II', data)
                            object_id = gpu_channels[fd].allocate_object(class_id, flags)
                            struct.pack_into('<Q', data, 8, object_id)
                        elif supported_gpfifo_alloc:
                            group = domain_owner.get(handle, handle)
                            if (fd not in gpu_channels or gpu_channels[fd].owner != group
                                    or not guest_range_has_permission(cpu, source, in_size, 1)
                                    or not writable_guest_range(cpu, target, out_size)):
                                raise ValueError('invalid GPU GPFIFO allocation buffer')
                            entries, flags, *reserved = struct.unpack('<8I', data)
                            if any(reserved):
                                raise ValueError('unsupported GPU GPFIFO allocation fields')
                            gpu_channels[fd].allocate_gpfifo(entries, flags, fd)
                            struct.pack_into('<II', data, 12, fd, 0)
                        elif supported_set_nvmap:
                            group = domain_owner.get(handle, handle)
                            nvmap_fd = struct.unpack('<I', data)[0]
                            if (fd not in gpu_channels or gpu_channels[fd].owner != group
                                    or nv_fds.get(nvmap_fd) != b'/dev/nvmap'
                                    or nv_fd_owners.get(nvmap_fd) != group
                                    or not guest_range_has_permission(cpu, source, in_size, 1)):
                                raise ValueError('invalid GPU channel NVMap binding')
                            gpu_channels[fd].bind_nvmap(nvmap_fd)
                            data = bytearray()
                        elif supported_as_bind:
                            group = domain_owner.get(handle, handle)
                            channel_fd = struct.unpack('<I', data)[0]
                            if (fd not in gpu_spaces or gpu_spaces[fd][0] != group
                                    or nv_fd_owners.get(fd) != group or channel_fd not in gpu_channels
                                    or not guest_range_has_permission(cpu, source, in_size, 1)):
                                raise ValueError('invalid GPU address-space channel binding')
                            gpu_channels[channel_fd].bind(group, gpu_spaces[fd][1])
                            data = bytearray()
                        elif supported_as_map:
                            group = domain_owner.get(handle, handle)
                            if (fd not in gpu_spaces or gpu_spaces[fd][0] != group
                                    or not guest_range_has_permission(cpu, source, in_size, 1)
                                    or not writable_guest_range(cpu, target, out_size)):
                                raise ValueError('invalid GPU mapping request buffer')
                            flags, kind, nv_id, page_size, offset, map_size, gpu_va = struct.unpack('<IIIIQQQ', data)
                            if (flags != 0 or kind not in (0, 0x11, 0xfe) or gpu_va != 0 or nv_id not in nv_maps
                                    or nv_map_owners.get(nv_id) != group):
                                raise ValueError(f'unsupported GPU mapping request flags=0x{flags:x} kind=0x{kind:x} id={nv_id} page=0x{page_size:x} offset=0x{offset:x} size=0x{map_size:x} va=0x{gpu_va:x}')
                            backing_size, guest_address = nv_maps[nv_id]
                            if not guest_address or not guest_range_has_permission(cpu, guest_address, backing_size, 1):
                                raise ValueError('invalid GPU mapping backing memory')
                            mapped_va = gpu_spaces[fd][1].map_dynamic(nv_id, guest_address, backing_size,
                                                                     page_size, offset, map_size, kind)
                            struct.pack_into('<Q', data, 32, mapped_va)
                        elif supported_as_alloc:
                            if (fd not in gpu_spaces or gpu_spaces[fd][0] != domain_owner.get(handle, handle)
                                    or not guest_range_has_permission(cpu, source, in_size, 1)
                                    or not writable_guest_range(cpu, target, out_size)):
                                raise ValueError('invalid GPU address-space allocation request')
                            pages, page_size, flags, padding, alignment = struct.unpack('<IIIIQ', data)
                            if padding:
                                raise ValueError('invalid GPU address-space allocation padding')
                            address = gpu_spaces[fd][1].reserve(pages, page_size, flags, alignment)
                            struct.pack_into('<Q', data, 16, address)
                        elif supported_gpu_metadata:
                            if not writable_guest_range(cpu, target, out_size):
                                raise ValueError('invalid GPU metadata output buffer')
                            data = bytearray(gpu_metadata_ioctl(ioctl, data, out_size))
                        elif supported_gpu_info:
                            if not guest_range_has_permission(cpu, source, in_size, 1) or not writable_guest_range(cpu, target, in_size):
                                raise ValueError('invalid GPU characteristics buffer')
                            data = bytearray(gpu_characteristics(data))
                        elif ioctl == 0xc0080101 and in_size == 8:
                            size = struct.unpack_from('<I', data)[0]
                            if size == 0 or size > (1 << 30):
                                raise ValueError('invalid NV allocation size')
                            nv_id = next(h for h in range(1, 4097) if h not in nv_maps)
                            nv_maps[nv_id] = (size, 0)
                            nv_map_owners[nv_id] = domain_owner.get(handle, handle)
                            struct.pack_into('<I', data, 4, nv_id)
                        elif ioctl == 0xc0200104 and in_size == 32:
                            nv_id = struct.unpack_from('<I', data)[0]
                            address = struct.unpack_from('<Q', data, 24)[0]
                            if (nv_id not in nv_maps or nv_map_owners.get(nv_id) != domain_owner.get(handle, handle)
                                    or nv_maps[nv_id][1] or address % 0x1000
                                    or not (0x400000000 <= address < 0x400000000 + heap_size)
                                    or nv_maps[nv_id][0] > 0x400000000 + heap_size - address
                                    or not guest_range_has_permission(cpu, address, nv_maps[nv_id][0], 1)):
                                raise ValueError('invalid NV allocation')
                            nv_maps[nv_id] = (nv_maps[nv_id][0], address)
                            nv_map_flags[nv_id] = struct.unpack_from('<I', data, 8)[0]
                        elif ioctl == 0xc0180105 and in_size == 24:
                            nv_id = struct.unpack_from('<I', data)[0]
                            if nv_id not in nv_maps or nv_map_owners.get(nv_id) != domain_owner.get(handle, handle):
                                raise ValueError('invalid NV handle')
                            spaces = [item[1] for item in gpu_spaces.values()] + [channel.address_space for channel in gpu_channels.values() if channel.address_space is not None]
                            if any(any(mapped_id == nv_id for _, mapped_id, _ in space.mappings.values()) for space in spaces):
                                raise ValueError('NV allocation remains GPU mapped')
                            allocation_size, address = nv_maps.pop(nv_id)
                            nv_map_owners.pop(nv_id)
                            flags = 1 if nv_map_flags.pop(nv_id, 0) & 2 else 0
                            struct.pack_into('<QII', data, 8, address, allocation_size, flags)
                        elif ioctl == 0xc008010e and in_size == 8:
                            nv_id = struct.unpack_from('<I', data, 4)[0]
                            if nv_id not in nv_maps or nv_map_owners.get(nv_id) != domain_owner.get(handle, handle):
                                raise ValueError('invalid NV handle')
                            struct.pack_into('<I', data, 0, nv_id)
                        else:
                            raise ValueError(f'unsupported NV ioctl 0x{ioctl:x}')
                        if data:
                            cpu.mem_write(target, bytes(data))
                    except (UcError, ValueError) as exc:
                        result['reason'] = str(exc)
                        cpu.emu_stop()
                        return
                if is_nv_open:
                    h0 = struct.unpack_from('<I', request)[0]
                    if ((h0 >> 20) & 15) != 1:
                        result['reason'] = 'unsupported NV open descriptor'
                        cpu.emu_stop()
                        return
                    size, low, desc = struct.unpack_from('<III', request, 8)
                    address = low | (((desc >> 28) & 15) << 32) | (((desc >> 2) & 0x3fffff) << 36)
                    if size > 64:
                        result['reason'] = 'unsupported NV device name length'
                        cpu.emu_stop()
                        return
                    try:
                        device = bytes(cpu.mem_read(address, size)).split(bytes([0]))[0]
                    except UcError:
                        result['reason'] = 'invalid NV device pointer'
                        cpu.emu_stop()
                        return
                    if device not in (b'/dev/nvmap', b'/dev/nvhost-ctrl', b'/dev/nvhost-ctrl-gpu', b'/dev/nvhost-gpu', b'/dev/nvhost-as-gpu'):
                        result['reason'] = f'unsupported NV device {device!r}'
                        cpu.emu_stop()
                        return
                    nv_fd = max(nv_fds, default=0) + 1
                    nv_fds[nv_fd] = device
                    nv_fd_owners[nv_fd] = domain_owner.get(handle, handle)
                    if device == b'/dev/nvhost-gpu':
                        gpu_channels[nv_fd] = GpuChannel(nv_fd_owners[nv_fd])
                        gpu_execution_states[nv_fd] = GpuExecutionState()
                is_vi_display = sessions.get(handle) == 'vi:m' and msg_type == 4 and command == 2
                is_vi_child = sessions.get(handle) == 'vi:display' and msg_type == 4 and command in (100, 101, 102, 103)
                is_display_id = sessions.get(handle) == 'vi:display' and msg_type == 4 and command == 1010
                is_open_layer = sessions.get(handle) == 'vi:display' and msg_type == 4 and command == 2020
                is_close_layer = sessions.get(handle) == 'vi:display' and msg_type == 4 and command == 2021
                is_close_display = sessions.get(handle) == 'vi:display' and msg_type == 4 and command == 1020
                if is_close_layer or is_close_display:
                    ident = struct.unpack_from('<Q', request, cmif + 16)[0]
                    key = 'layer' if is_close_layer else 'display'
                    if ident != 1 or not vi_state[key] or (is_close_display and vi_state['layer']):
                        result['reason'] = f'unsupported VI close {key} id={ident}'
                        cpu.emu_stop()
                        return
                    vi_state[key] = False
                is_layer_scale = sessions.get(handle) == 'vi:display' and msg_type == 4 and command == 2101
                is_binder_adjust = sessions.get(handle) == 'vi:child:100' and msg_type == 4 and command == 1
                is_vi_vsync = sessions.get(handle) == 'vi:display' and msg_type == 4 and command == 5202
                is_binder_event = sessions.get(handle) == 'vi:child:100' and msg_type == 4 and command == 2
                transaction = struct.unpack_from('<I', request, cmif + 20)[0] if 0 <= cmif <= len(request) - 24 else -1
                is_binder_connect = (sessions.get(handle) == 'vi:child:100' and msg_type == 4
                                     and command == 3 and transaction == 10)
                is_binder_disconnect = sessions.get(handle) == 'vi:child:100' and msg_type == 4 and command == 3 and transaction == 11
                if is_binder_disconnect:
                    try:
                        api = binder_queue_slot(cpu, request)
                        if api not in (1, 2, 3, 4):
                            raise ValueError('unsupported binder disconnect API')
                        display_queue = BufferQueue()
                    except (UcError, ValueError, struct.error) as exc:
                        result['reason'] = f'binder disconnect: {exc}'
                        cpu.emu_stop()
                        return
                is_binder_preallocate = (sessions.get(handle) == 'vi:child:100' and command == 3 and transaction == 14)
                is_binder_dequeue = (sessions.get(handle) == 'vi:child:100' and command == 3 and transaction == 3)
                is_binder_request = (sessions.get(handle) == 'vi:child:100' and command == 3 and transaction == 1)
                is_binder_queue = sessions.get(handle) == 'vi:child:100' and command == 3 and transaction == 7
                if is_binder_queue:
                    try:
                        slot = binder_queue_slot(cpu, request)
                        buf = display_queue.queue(slot)
                        frame_count += 1
                        if frame_count == 1 and capture_first is not None:
                            save_frame(cpu, buf, nv_maps[buf[0]], capture_first)
                        if window:
                            quit_requested, mask = window.draw_and_poll(frame_pixels(cpu, buf, nv_maps[buf[0]]), buf[1], buf[2])
                            if quit_requested and not request_guest_exit():
                                result['reason'] = f'window closed after {frame_count} frames (no applet exit event)'
                                cpu.emu_stop()
                                return
                        else:
                            if pressed:
                                move_index += 1
                            if not moves or move_index >= len(moves):
                                if capture is not None:
                                    save_frame(cpu, buf, nv_maps[buf[0]], capture)
                                result['reason'] = f'captured frame {frame_count} after {move_index} inputs'
                                cpu.emu_stop()
                                return
                            mask = 0 if pressed else buttons[moves[move_index]]
                            pressed = not pressed
                        if hid_address is None:
                            raise ValueError('controller memory not mapped')
                        update_hid(cpu, mask)
                    except (UcError, ValueError, OSError, RuntimeError) as exc:
                        result['reason'] = f'frame capture failed: {exc}'
                        cpu.emu_stop()
                        return
                if is_binder_preallocate:
                    first, low = struct.unpack_from('<II', request, 8)
                    address = low | (((first >> 12) & 15) << 32) | (((first >> 6) & 63) << 36)
                    size = first >> 16
                    try:
                        raw = bytes(cpu.mem_read(address, min(size, 1024)))
                        magic = raw.find(b'RFBG')
                        if magic < 16 or magic + 132 > len(raw):
                            raise ValueError('invalid flattened graphic buffer')
                        slot = struct.unpack_from('<I', raw, magic - 16)[0]
                        nv_id = struct.unpack_from('<I', raw, magic + 44)[0]
                        width, height = struct.unpack_from('<II', raw, magic + 92)
                        pitch = struct.unpack_from('<I', raw, magic + 112)[0]
                        plane_offset = struct.unpack_from('<I', raw, magic + 120)[0]
                        pixel_format = struct.unpack_from('<I', raw, magic + 16)[0]
                        if (nv_id not in nv_maps or not nv_maps[nv_id][1] or pixel_format not in (1, 5)
                                or not 1 <= width <= 4096 or not 1 <= height <= 2160):
                            raise ValueError(f'unsupported graphic buffer nv={nv_id} allocation={nv_maps.get(nv_id)} format={pixel_format} width={width} height={height}')
                        block_height = struct.unpack_from('<I', raw, magic + 128)[0]
                        display_queue.register(slot, (nv_id, width, height, pitch, plane_offset, block_height, pixel_format))
                    except (UcError, ValueError, struct.error) as exc:
                        result['reason'] = f'binder buffer: {exc}'
                        cpu.emu_stop()
                        return
                is_hid_setup = msg_type == 4 and ((sessions.get(handle) == 'hid' and command in (11, 100, 102, 103, 109, 124, 201, 204, 206))
                                                or (sessions.get(handle) == 'hid:vibration' and command == 0))
                is_vibration_permitted = sessions.get(handle) == 'hid' and msg_type == 4 and command == 205
                is_vibration_list = sessions.get(handle) == 'hid' and msg_type == 4 and command == 203
                is_hid_resource = sessions.get(handle) == 'hid' and msg_type == 4 and command == 0
                is_hid_shared = sessions.get(handle) == 'hid:resource' and msg_type == 4 and command == 0
                is_time_resource = sessions.get(handle) in ('time:u', 'time:s') and msg_type == 4 and command in range(5)
                is_time_shared = sessions.get(handle) in ('time:u', 'time:s') and msg_type == 4 and command == 20
                is_clock = sessions.get(handle) in ('time:0', 'time:1', 'time:2') and msg_type == 4 and command == 0
                is_timezone = sessions.get(handle) == 'time:3' and msg_type == 4 and command == 101
                is_domain_convert = sessions.get(handle) in ('appletAE', 'fsp-srv') and msg_type == 5 and command == 0
                object_id = struct.unpack_from('<I', request, cmif - 12)[0] if cmif >= 16 else 0
                owner = domain_owner.get(handle, handle)
                object_kind = domain_objects.get((owner, object_id))
                is_legacy = sessions.get(handle, '').startswith('legacy:') or (sessions.get(handle) == 'fsp-srv' and (owner, 1) not in domain_objects)
                if sessions.get(handle, '').startswith('legacy:'):
                    object_kind = sessions[handle][7:]
                    object_id = 0
                elif is_legacy:
                    object_kind = 'fsp-srv'
                is_clone = sessions.get(handle) in ('fsp-srv', 'nvdrv:a', 'bsd:s', 'bsd:u') and msg_type == 5 and command in (2, 4)
                is_fs_init = object_kind == 'fsp-srv' and command == 1 and msg_type == 4
                is_fs_open = object_kind == 'fsp-srv' and command == 18 and msg_type == 4
                is_save_reader_open = object_kind == 'fsp-srv' and command == 61 and msg_type == 4
                is_save_reader_read = object_kind == 'save_reader' and command == 0 and msg_type == 4
                is_save_create = object_kind == 'fsp-srv' and command == 22 and msg_type == 4
                is_save_open = object_kind == 'fsp-srv' and command == 51 and msg_type == 4
                is_save_commit = object_kind == 'savecard' and command == 10 and msg_type == 4
                if (is_save_reader_open or is_save_create or is_save_open) and not saves_loaded:
                    try:
                        virtual_saves.update(save_store.load())
                        committed_saves = copy.deepcopy(virtual_saves)
                        saves_loaded = True
                    except (OSError, ValueError) as exc:
                        result['reason'] = f'save storage unavailable: {exc}'
                        cpu.emu_stop()
                        return
                if is_save_commit:
                    if os.environ.get('SOJU_SWITCH_TRACE_FS'):
                        print('soju-switch-fs: commit save', flush=True)
                    try:
                        save_attr = domain_save_attrs[(owner, object_id)]
                        snapshot = dict(committed_saves)
                        snapshot[save_attr] = copy.deepcopy(virtual_saves[save_attr])
                        save_store.save(snapshot)
                        committed_saves = snapshot
                    except (KeyError, OSError, ValueError) as exc:
                        result['reason'] = f'save commit failed: {exc}'
                        cpu.emu_stop()
                        return
                save_read_count = 0
                if is_save_reader_read:
                    try:
                        reader = save_readers[(owner, object_id)]
                        h0, h1 = struct.unpack_from('<II', request)
                        if h1 & 0x80000000 or ((h0 >> 24) & 15) != 1:
                            raise ValueError('unsupported save reader descriptor')
                        pos = 8 + ((h0 >> 16) & 15) * 8 + ((h0 >> 20) & 15) * 12
                        size_low, low, packed = struct.unpack_from('<III', request, pos)
                        address = low | (((packed >> 28) & 15) << 32) | (((packed >> 2) & 0x3fffff) << 36)
                        capacity = size_low | (((packed >> 24) & 15) << 32)
                        if capacity > 8 * 1024 * 1024:
                            raise ValueError('save reader buffer too large')
                        pending = reader['entries'][reader['position']:reader['position'] + capacity // 0x60]
                        output = bytearray(len(pending) * 0x60)
                        for i, (save_attr, saved) in enumerate(pending):
                            offset = i * 0x60
                            struct.pack_into('<QBB', output, offset, saved['id'], saved['space'], save_attr[0x20])
                            output[offset + 0x10:offset + 0x20] = save_attr[8:24]
                            output[offset + 0x20:offset + 0x28] = save_attr[0x18:0x20]
                            output[offset + 0x28:offset + 0x30] = save_attr[:8]
                            struct.pack_into('<Q', output, offset + 0x30, saved['size'])
                            output[offset + 0x38:offset + 0x3b] = save_attr[0x22:0x24] + save_attr[0x21:0x22]
                        if not writable_guest_range(cpu, address, len(output)):
                            raise ValueError('save reader output is not writable')
                        if output:
                            cpu.mem_write(address, bytes(output))
                        save_read_count = len(pending)
                        reader['position'] += save_read_count
                    except (KeyError, UcError, ValueError, struct.error) as exc:
                        result['reason'] = f'save reader: {exc}'
                        cpu.emu_stop()
                        return
                if is_save_create or is_save_open:
                    if os.environ.get('SOJU_SWITCH_TRACE_FS'):
                        print(f'soju-switch-fs: {"create" if is_save_create else "open"} save', flush=True)
                    pos = cmif + (16 if is_save_create else 24)
                    attr = request[pos:pos + 0x40]
                    if len(attr) != 0x40 or attr[0x20] != 1 or attr[8:24] != struct.pack('<QQ', 1, 0) or any(attr[0x24:]):
                        result['reason'] = 'unsupported save data identity'
                        cpu.emu_stop()
                        return
                    if is_save_create:
                        data_size, journal_size = struct.unpack_from('<qq', request, pos + 0x40)
                        if request[pos + 0x64] != 1:
                            result['reason'] = 'unsupported save data space'
                            cpu.emu_stop()
                            return
                        if data_size < 0 or journal_size < 0 or data_size > 128 * 1024 * 1024:
                            result['reason'] = f'unsupported save data size data={data_size} journal={journal_size}'
                            cpu.emu_stop()
                            return
                        if attr not in virtual_saves and len(virtual_saves) >= 16:
                            result['reason'] = 'virtual save count exceeds limit'
                            cpu.emu_stop()
                            return
                        if attr not in virtual_saves:
                            new_save = {'dirs': {'/'}, 'files': MemoryFiles(),
                                        'id': max((v['id'] for v in virtual_saves.values()), default=0) + 1,
                                        'space': request[pos + 0x64], 'size': data_size}
                            try:
                                snapshot = dict(committed_saves)
                                snapshot[attr] = copy.deepcopy(new_save)
                                save_store.save(snapshot)
                                committed_saves = snapshot
                                virtual_saves[attr] = new_save
                            except (OSError, ValueError) as exc:
                                result['reason'] = f'save creation failed: {exc}'
                                cpu.emu_stop()
                                return
                    elif attr not in virtual_saves or request[cmif + 16] != virtual_saves[attr]['space']:
                        result['reason'] = 'requested virtual save does not exist'
                        cpu.emu_stop()
                        return
                is_fs_path = object_kind in ('sdcard', 'savecard') and command in (0, 2, 7, 8, 9, 14) and msg_type == 4
                is_file_read = object_kind == 'self_file' and command == 0 and msg_type == 4
                is_file_size = object_kind == 'self_file' and command == 4 and msg_type == 4
                is_file_open = False
                is_memory_file_open = False
                is_memory_file_op = object_kind == 'memory_file' and command in (0, 1, 2, 3, 4) and msg_type == 4
                is_directory_open = False
                directory = None
                is_directory_op = object_kind == 'directory' and command in (0, 1) and msg_type == 4
                file_value = 0
                fs_result = 0
                if is_fs_path:
                    selected_save = domain_saves.get((owner, object_id))
                    fs_dirs = selected_save['dirs'] if selected_save is not None else directories
                    fs_files = selected_save['files'] if selected_save is not None else memory_files
                    self_path = '/switch/soju-current.nro' if selected_save is None else None
                    h0 = struct.unpack_from('<I', request)[0]
                    if ((h0 >> 16) & 15) != 1:
                        result['reason'] = 'unsupported FS path descriptor'
                        cpu.emu_stop()
                        return
                    first, low = struct.unpack_from('<II', request, 8)
                    size = first >> 16
                    address = low | (((first >> 12) & 15) << 32) | (((first >> 6) & 63) << 36)
                    try:
                        raw_path = bytes(cpu.mem_read(address, min(size, 0x301)))
                        if bytes(1) not in raw_path:
                            raise ValueError('unterminated path')
                        path = raw_path.split(bytes(1), 1)[0].decode('utf-8')
                    except (UcError, UnicodeDecodeError, ValueError):
                        result['reason'] = 'invalid FS path'
                        cpu.emu_stop()
                        return
                    if not path.startswith('/') or any(part in ('.', '..') for part in path.split('/')):
                        result['reason'] = 'unsupported FS path'
                        cpu.emu_stop()
                        return
                    path = '/' + '/'.join(part for part in path.split('/') if part)
                    if os.environ.get('SOJU_SWITCH_TRACE_FS'):
                        print(f'soju-switch-fs: {object_kind} cmd={command} path={path!r}', flush=True)
                    if command == 0:
                        option, size = struct.unpack_from('<I4xQ', request, cmif + 16)
                        parent = path.rpartition('/')[0] or '/'
                        if path in fs_files.files or path in fs_dirs or path == self_path:
                            fs_result = 0x402
                        elif parent not in fs_dirs:
                            fs_result = 0x202
                        else:
                            try:
                                if option != 0:
                                    raise ValueError('unsupported create option')
                                fs_files.create(path, size)
                            except ValueError as exc:
                                result['reason'] = f'file service: {exc}'
                                cpu.emu_stop()
                                return
                    elif command == 2:
                        parent = path.rpartition('/')[0] or '/'
                        fs_result = 0x402 if path in fs_dirs or path in fs_files.files or path == self_path else 0x202 if parent not in fs_dirs else 0
                        if fs_result == 0:
                            if len(fs_dirs) >= 256:
                                result['reason'] = 'virtual directory count exceeds limit'
                                cpu.emu_stop()
                                return
                            fs_dirs.add(path)
                    elif command in (7, 14):
                        fs_result = 0 if path in fs_dirs or path in fs_files.files or path == self_path else 0x202
                        file_value = 1 if path in fs_files.files or path == self_path else 0
                    else:
                        mode = struct.unpack_from('<I', request, cmif + 16)[0]
                        is_file_open = command == 8 and path == self_path and mode == 1
                        is_memory_file_open = command == 8 and path in fs_files.files and bool(mode & 3) and not (mode & ~7)
                        if command == 9 and path in fs_dirs:
                            try:
                                directory = Directory(path, mode, fs_dirs, input_path.stat().st_size, fs_files.files, selected_save is None)
                                is_directory_open = True
                            except (OSError, ValueError) as exc:
                                result['reason'] = f'directory service: {exc}'
                                cpu.emu_stop()
                                return
                        fs_result = 0 if is_file_open or is_memory_file_open or is_directory_open else 0x202
                if is_directory_op:
                    try:
                        directory = domain_directories[(owner, object_id)]
                        if command == 1:
                            file_value = len(directory.entries)
                        else:
                            h0, h1 = struct.unpack_from('<II', request)
                            if h1 & 0x80000000 or ((h0 >> 24) & 15) != 1:
                                raise ValueError('unsupported directory output descriptor')
                            pos = 8 + ((h0 >> 16) & 15) * 8 + ((h0 >> 20) & 15) * 12
                            size_low, low, packed = struct.unpack_from('<III', request, pos)
                            address = low | (((packed >> 28) & 15) << 32) | (((packed >> 2) & 0x3fffff) << 36)
                            size = size_low | (((packed >> 24) & 15) << 32)
                            data, file_value = directory.read(size)
                            if not writable_guest_range(cpu, address, len(data)):
                                raise ValueError('directory output is not writable guest memory')
                            if data:
                                cpu.mem_write(address, data)
                    except (KeyError, ValueError, UcError, struct.error) as exc:
                        result['reason'] = f'directory service: {exc}'
                        cpu.emu_stop()
                        return
                if is_memory_file_op:
                    try:
                        file_path, file_mode, files = domain_files[(owner, object_id)]
                        if os.environ.get('SOJU_SWITCH_TRACE_FS'):
                            print(f'soju-switch-fs: file cmd={command} path={file_path!r}', flush=True)
                        if command == 4:
                            file_value = len(files.files[file_path])
                        elif command == 3:
                            if not file_mode & 2:
                                raise ValueError('file not opened for writing')
                            files.resize(file_path, struct.unpack_from('<q', request, cmif + 16)[0])
                        elif command in (0, 1):
                            option, _, offset, length = struct.unpack_from('<IIqQ', request, cmif + 16)
                            if not (file_mode & (1 if command == 0 else 2)) or option not in ((0,) if command == 0 else (0, 1)):
                                raise ValueError('unsupported file mode or option')
                            h0, h1 = struct.unpack_from('<II', request)
                            is_read = command == 0
                            if h1 & 0x80000000 or ((h0 >> (24 if is_read else 20)) & 15) != 1:
                                raise ValueError('unsupported file descriptor')
                            pos = 8 + ((h0 >> 16) & 15) * 8 + (((h0 >> 20) & 15) * 12 if is_read else 0)
                            size_low, low, packed = struct.unpack_from('<III', request, pos)
                            address = low | (((packed >> 28) & 15) << 32) | (((packed >> 2) & 0x3fffff) << 36)
                            size = size_low | (((packed >> 24) & 15) << 32)
                            if length > size or length > 8 * 1024 * 1024:
                                raise ValueError('invalid file buffer size')
                            if is_read:
                                data = files.read(file_path, offset, length)
                                if not writable_guest_range(cpu, address, len(data)):
                                    raise ValueError('file output is not writable guest memory')
                                if data:
                                    cpu.mem_write(address, data)
                                file_value = len(data)
                            else:
                                if not guest_range_has_permission(cpu, address, length, 1):
                                    raise ValueError('file input is not readable guest memory')
                                data = bytes(cpu.mem_read(address, length)) if length else b''
                                files.write(file_path, offset, data, bool(file_mode & 4))
                    except (KeyError, ValueError, UcError, struct.error) as exc:
                        result['reason'] = f'file service: {exc}'
                        cpu.emu_stop()
                        return
                if is_file_read or is_file_size:
                    try:
                        if is_file_size:
                            file_value = input_path.stat().st_size
                        else:
                            option, _, offset, length = struct.unpack_from('<IIqQ', request, cmif + 16)
                            h0, h1 = struct.unpack_from('<II', request)
                            if h1 & 0x80000000 or ((h0 >> 24) & 15) != 1:
                                raise ValueError('unsupported file output descriptor')
                            pos = 8 + ((h0 >> 16) & 15) * 8 + ((h0 >> 20) & 15) * 12
                            size_low, low, packed = struct.unpack_from('<III', request, pos)
                            address = low | (((packed >> 28) & 15) << 32) | (((packed >> 2) & 0x3fffff) << 36)
                            size = size_low | (((packed >> 24) & 15) << 32)
                            if option != 0 or offset < 0 or length > size or length > 8 * 1024 * 1024:
                                raise ValueError('invalid file read')
                            with input_path.open('rb') as source:
                                source.seek(offset)
                                data = source.read(length)
                            if not writable_guest_range(cpu, address, len(data)):
                                raise ValueError('file output is not writable guest memory')
                            if data:
                                cpu.mem_write(address, data)
                            file_value = len(data)
                    except (OSError, UcError, ValueError, struct.error) as exc:
                        result['reason'] = f'file service: {exc}'
                        cpu.emu_stop()
                        return
                is_legacy_proxy = sessions.get(handle) == 'appletAE' and object_kind is None and command == 200 and msg_type == 4
                is_proxy = object_kind == 'appletAE' and command == 201 and msg_type == 4
                proxy_services = {0: 'common_state', 1: 'self_controller', 2: 'window_controller',
                                  3: 'audio_controller', 4: 'display_controller', 10: 'winding',
                                  11: 'library_creator', 20: 'library_self', 21: 'common_functions',
                                  22: 'home_menu', 23: 'global_state', 1000: 'debug'}
                is_proxy_service = object_kind == 'applet_proxy' and command in proxy_services and msg_type == 4
                is_legacy_proxy_service = is_proxy_service and sessions.get(handle, '').startswith('legacy:')
                is_applet_create = object_kind == 'library_creator' and command == 0 and msg_type == 4
                if is_applet_create:
                    applet_id, mode = struct.unpack_from('<II', request, cmif + 16)
                    if applet_id != 0x10 or mode != 0:
                        result['reason'] = f'unsupported library applet id=0x{applet_id:x} mode={mode}'
                        cpu.emu_stop()
                        return
                is_applet_state_event = object_kind == 'player_select' and command == 0 and msg_type == 4
                is_storage_create = object_kind == 'library_creator' and command == 10 and msg_type == 4
                is_storage_open = object_kind == 'applet_storage' and command == 0 and msg_type == 4
                is_storage_io = object_kind == 'storage_accessor' and command in (0, 10, 11) and msg_type == 4
                is_applet_push = object_kind == 'player_select' and command == 100 and msg_type == 4
                is_applet_start = object_kind == 'player_select' and command == 10 and msg_type == 4
                is_applet_result = object_kind == 'player_select' and command == 30 and msg_type == 4
                is_applet_pop = object_kind == 'player_select' and command == 101 and msg_type == 4
                storage_value = 0
                output_storage = None
                if is_storage_io:
                    try:
                        storage = applet_objects[(owner, object_id)]
                        if command == 0:
                            storage_value = len(storage)
                        else:
                            offset = struct.unpack_from('<q', request, cmif + 16)[0]
                            h0, h1 = struct.unpack_from('<II', request)
                            if h1 & 0x80000000:
                                raise ValueError('unsupported storage descriptor')
                            nstatic, nsend, nrecv, nexch = ((h0 >> shift) & 15 for shift in (16, 20, 24, 28))
                            if command == 10 and nstatic:
                                first, low = struct.unpack_from('<II', request, 8)
                                address = low | (((first >> 12) & 15) << 32) | (((first >> 6) & 63) << 36)
                                size = first >> 16
                            elif command == 11 and ((h1 >> 10) & 15) == 3:
                                pos = 8 + nstatic * 8 + (nsend + nrecv + nexch) * 12 + (h1 & 0x3ff) * 4
                                low, high_size = struct.unpack_from('<II', request, pos)
                                address = low | ((high_size & 0xffff) << 32)
                                size = high_size >> 16
                            else:
                                raise ValueError('unsupported applet storage buffer')
                            if size == 0 and ((command == 10 and nsend == 1) or (command == 11 and nrecv == 1)):
                                pos = 8 + nstatic * 8 + (nsend * 12 if command == 11 else 0)
                                size_low, low, packed = struct.unpack_from('<III', request, pos)
                                address = low | (((packed >> 28) & 15) << 32) | (((packed >> 2) & 0x3fffff) << 36)
                                size = size_low | (((packed >> 24) & 15) << 32)
                            if offset < 0 or offset > len(storage) or size > len(storage) - offset:
                                raise ValueError('applet storage range outside allocation')
                            if command == 10:
                                if not guest_range_has_permission(cpu, address, size, 1):
                                    raise ValueError('applet input is not readable')
                                storage[offset:offset + size] = cpu.mem_read(address, size)
                            else:
                                if not writable_guest_range(cpu, address, size):
                                    raise ValueError('applet output is not writable')
                                cpu.mem_write(address, bytes(storage[offset:offset + size]))
                    except (KeyError, ValueError, UcError, struct.error) as exc:
                        result['reason'] = f'applet storage: {exc}'
                        cpu.emu_stop()
                        return
                if is_applet_push:
                    try:
                        count = request[cmif - 15]
                        data_size = struct.unpack_from('<H', request, cmif - 14)[0]
                        if count != 1 or cmif + data_size + 4 > len(request):
                            raise ValueError('invalid applet input object')
                        input_id = struct.unpack_from('<I', request, cmif + data_size)[0]
                        if domain_objects.get((owner, input_id)) != 'applet_storage':
                            raise ValueError('invalid applet input storage')
                        applet_objects[(owner, object_id)]['inputs'].append(bytes(applet_objects[(owner, input_id)]))
                    except (KeyError, ValueError, struct.error) as exc:
                        result['reason'] = f'applet push: {exc}'
                        cpu.emu_stop()
                        return
                if is_applet_start:
                    try:
                        state = applet_objects[(owner, object_id)]
                        if len(state['inputs']) != 2 or state['event'] is None or state.get('completed'):
                            raise ValueError('invalid player selector state')
                        common, settings = state['inputs']
                        if len(common) != 0x20 or struct.unpack_from('<III', common) != (1, 0x20, 0x20000) or len(settings) != 0xa0:
                            raise ValueError('unsupported player selector argument version')
                        uid = struct.pack('<QQ', 1, 0)  # one offline local user; no Nintendo account
                        if len(settings) < 0x98 or struct.unpack_from('<I', settings)[0] != 0 or settings[0x90] or any(settings[i:i+16] == uid for i in range(8, 0x88, 16)):
                            raise ValueError('unsupported player selection settings')
                        state['output'] = bytearray(struct.pack('<IIQQ', 0, 0, 1, 0))
                        state['completed'] = True
                        events[state['event']] = True
                    except (KeyError, ValueError, struct.error) as exc:
                        result['reason'] = f'player selection: {exc}'
                        cpu.emu_stop()
                        return
                if is_applet_result and not applet_objects[(owner, object_id)].get('completed'):
                    result['reason'] = 'player selection not complete'
                    cpu.emu_stop()
                    return
                if is_applet_pop:
                    state = applet_objects[(owner, object_id)]
                    output_storage = state.pop('output', None)
                    if output_storage is None:
                        result['reason'] = 'player selection has no output'
                        cpu.emu_stop()
                        return
                if is_storage_create:
                    storage_size = struct.unpack_from('<q', request, cmif + 16)[0]
                    if storage_size < 0 or storage_size > 4096:
                        result['reason'] = 'unsupported applet storage size'
                        cpu.emu_stop()
                        return
                is_aruid = msg_type == 4 and ((object_kind == 'window_controller' and command == 1)
                                                or (object_kind == 'self_controller' and command == 40))
                is_event = msg_type == 4 and ((object_kind == 'self_controller' and command in (9, 91))
                                               or (object_kind == 'common_state' and command == 0))
                is_applet_message = object_kind == 'common_state' and command == 1 and msg_type == 4
                is_mode = object_kind == 'common_state' and command in (5, 6, 9) and msg_type == 4
                is_notify = object_kind == 'self_controller' and command in (11, 12) and msg_type == 4
                is_launch_info = msg_type == 4 and ((object_kind == 'library_self' and command == 11)
                                                    or (object_kind == 'winding' and command == 0))
                if is_binder_connect or is_binder_disconnect or is_binder_preallocate or is_binder_dequeue or is_binder_request or is_binder_queue:
                    h0, h1 = struct.unpack_from('<II', request)
                    recv_count = (h1 >> 10) & 15
                    descriptor = (8 + ((h0 >> 16) & 15) * 8 + ((h0 >> 20) & 15) * 12
                                  + ((h0 >> 24) & 15) * 12 + ((h0 >> 28) & 15) * 12
                                  + (h1 & 0x3ff) * 4)
                    if (h1 & 0x80000000) or recv_count != 3 or descriptor + 8 > len(request):
                        result['reason'] = 'unsupported binder output descriptor'
                        cpu.emu_stop()
                        return
                    low, high_size = struct.unpack_from('<II', request, descriptor)
                    address, size = low | ((high_size & 0xffff) << 32), high_size >> 16
                    if size == 0 and ((h0 >> 24) & 15) == 1:
                        offset = 8 + ((h0 >> 16) & 15) * 8 + ((h0 >> 20) & 15) * 12
                        low_size, low_address, packed = struct.unpack_from('<III', request, offset)
                        address = low_address | (((packed >> 28) & 15) << 32) | (((packed >> 2) & 0x3fffff) << 36)
                        size = low_size | (((packed >> 24) & 15) << 32)
                    if is_binder_connect or is_binder_queue:
                        width, height = (buf[1], buf[2]) if is_binder_queue else (1280, 720)
                        payload = struct.pack('<IIIII', width, height, 0, 0, 0)
                    elif is_binder_dequeue:
                        try:
                            slot = display_queue.dequeue()
                        except ValueError as exc:
                            result['reason'] = str(exc)
                            cpu.emu_stop()
                            return
                        payload = struct.pack('<iiII', slot, 1, 36, 0) + bytes(36) + struct.pack('<i', 0)
                    elif is_binder_request:
                        payload = struct.pack('<ii', 0, 0)
                    elif is_binder_disconnect:
                        payload = struct.pack('<i', 0)
                    else:
                        payload = b''
                    parcel = struct.pack('<IIII', len(payload), 16, 0, 16 + len(payload)) + payload
                    if size < len(parcel):
                        result['reason'] = 'binder output buffer too small'
                        cpu.emu_stop()
                        return
                    try:
                        cpu.mem_write(address, parcel)
                    except UcError:
                        result['reason'] = f'invalid binder output buffer 0x{address:x}'
                        cpu.emu_stop()
                        return
                if is_open_layer:
                    h0, h1 = struct.unpack_from('<II', request, 0)
                    # One output map-alias descriptor with the PID special header.
                    if ((h0 >> 24) & 15) != 1 or not (h1 & 0x80000000):
                        result['reason'] = 'unsupported VI layer buffer descriptor'
                        cpu.emu_stop()
                        return
                    low_size, low_address, descriptor = struct.unpack_from('<III', request, 20)
                    target = low_address | (((descriptor >> 28) & 15) << 32) | (((descriptor >> 2) & 0x3fffff) << 36)
                    size = low_size | (((descriptor >> 24) & 15) << 32)
                    parcel = struct.pack('<IIIIIII', 12, 16, 0, 28, 0, 0, 1)
                    if size < len(parcel):
                        result['reason'] = 'VI layer buffer too small'
                        cpu.emu_stop()
                        return
                    try:
                        cpu.mem_write(target, parcel)
                        vi_state['layer'] = True
                    except UcError:
                        result['reason'] = f'invalid VI layer buffer 0x{target:x}'
                        cpu.emu_stop()
                        return
                if is_firmware:
                    header1 = struct.unpack_from('<I', request, 4)[0]
                    recv_mode = (header1 >> 10) & 15
                    data_words = header1 & 0x3ff
                    descriptor = 8 + data_words * 4
                    if recv_mode != 3 or descriptor + 8 > len(request):
                        result['reason'] = 'unsupported firmware receive descriptor'
                        cpu.emu_stop()
                        return
                    low, high_size = struct.unpack_from('<II', request, descriptor)
                    address = low | ((high_size & 0xffff) << 32)
                    size = high_size >> 16
                    if size != 0x100:
                        result['reason'] = f'unsupported firmware buffer size {size}'
                        cpu.emu_stop()
                        return
                    firmware = bytearray(0x100)
                    firmware[:3] = bytes([17, 0, 0])  # simulated HOS 17.0.0
                    try:
                        cpu.mem_write(address, bytes(firmware))
                    except UcError:
                        result['reason'] = f'invalid firmware buffer 0x{address:x}'
                        cpu.emu_stop()
                        return
                if (is_query or is_register or is_open or is_bsd_init or is_font or is_account_registration or is_apm_session or is_apm_mode or is_firmware or is_domain_convert
                        or is_proxy or is_legacy_proxy or is_proxy_service or is_applet_message or is_applet_create or is_applet_state_event or is_storage_create or is_storage_open or is_storage_io or is_applet_push or is_applet_start or is_applet_result or is_applet_pop or is_aruid or is_event or is_mode or is_notify
                        or is_launch_info or is_hid_resource or is_hid_shared or is_time_resource or is_time_shared or is_clock or is_timezone or is_fs_init or is_fs_open or is_save_reader_open or is_save_reader_read or is_save_create or is_save_open or is_save_commit or is_clone or is_vi_display or is_vi_child or is_display_id or is_open_layer or is_close_layer or is_close_display or is_layer_scale or is_binder_adjust or is_binder_event or is_vi_vsync or is_binder_connect or is_binder_disconnect or is_binder_preallocate or is_binder_dequeue or is_binder_request or is_binder_queue or is_fs_path or is_file_read or is_file_size or is_memory_file_op or is_directory_op or is_hid_setup or is_vibration_list or is_vibration_permitted or is_nv_init or is_nv_open or is_nv_ioctl or is_nv_close or is_nv_event):
                    response = bytearray(0x100)
                    special = (is_font and command == 4) or is_apm_session or is_legacy_proxy or is_legacy_proxy_service or (is_legacy and (is_fs_open or is_file_open or is_memory_file_open or is_directory_open)) or is_open or is_applet_state_event or is_event or is_hid_resource or is_hid_shared or is_time_resource or is_time_shared or is_clone or is_vi_display or is_vi_child or is_binder_event or is_vi_vsync or is_vibration_list or is_nv_event
                    if is_clone:
                        new_handle = next(h for h in range(0x103, 0x200) if h not in sessions and h not in domain_owner.values())
                        sessions[new_handle] = sessions[handle]
                        domain_owner[new_handle] = owner
                    struct.pack_into('<II', response, 0, 0, (0x80000000 if special else 0) | 12)
                    if is_clone:
                        struct.pack_into('<II', response, 8, 0x20, new_handle)
                    if is_font and command == 4:
                        shared_handles.add(0x302)
                        struct.pack_into('<II', response, 8, 2, 0x302)
                    if is_hid_shared or is_time_shared:
                        shared_handle = 0x300 if is_hid_shared else 0x301
                        shared_handles.add(shared_handle)
                        struct.pack_into('<II', response, 8, 2, shared_handle)
                    if is_hid_resource or is_time_resource or is_vi_display or is_vi_child or is_apm_session or is_vibration_list:
                        new_handle = next(h for h in range(0x103, 0x200) if h not in sessions and h not in domain_owner.values())
                        sessions[new_handle] = ('hid:vibration' if is_vibration_list else 'apm:session' if is_apm_session else f'vi:child:{command}' if is_vi_child else
                                                'vi:display' if is_vi_display else
                                                'hid:resource' if is_hid_resource else f'time:{command}')
                        struct.pack_into('<II', response, 8, 0x20, new_handle)
                    if is_event or is_binder_event or is_applet_state_event or is_vi_vsync or is_nv_event:
                        event_handle = next(h for h in range(0x200, 0x300) if h not in events)
                        events[event_handle] = bool(is_binder_event or (object_kind == 'self_controller' and command == 9))
                        if is_nv_event:
                            aliases = gpu_channels[event_fd].error_events
                            events[event_handle] = any(events.get(h, False) for h in aliases)
                            aliases.append(event_handle)
                            event_aliases[event_handle] = aliases
                        if object_kind == 'common_state' and is_event:
                            applet_message_events.add(event_handle)
                            events[event_handle] = bool(applet_messages)
                        if is_vi_vsync:
                            vsync_events[event_handle] = time.monotonic_ns() + 16666667
                        if is_applet_state_event:
                            applet_objects[(owner, object_id)]['event'] = event_handle
                        struct.pack_into('<II', response, 8, 2, event_handle)
                    if is_legacy and (is_fs_open or is_file_open or is_memory_file_open or is_directory_open):
                        new_handle = next(h for h in range(0x103, 0x200) if h not in sessions and h not in domain_owner.values())
                        sessions[new_handle] = ('legacy:sdcard' if is_fs_open else 'legacy:self_file' if is_file_open else
                                                'legacy:memory_file' if is_memory_file_open else 'legacy:directory')
                        if is_memory_file_open:
                            domain_files[(new_handle, 0)] = (path, mode, fs_files)
                        if is_directory_open:
                            domain_directories[(new_handle, 0)] = directory
                        struct.pack_into('<II', response, 8, 0x20, new_handle)
                    if is_legacy_proxy_service:
                        new_handle = next(h for h in range(0x103, 0x200) if h not in sessions and h not in domain_owner.values())
                        sessions[new_handle] = 'legacy:' + proxy_services[command]
                        struct.pack_into('<II', response, 8, 0x20, new_handle)
                    if is_legacy_proxy:
                        new_handle = next(h for h in range(0x103, 0x200) if h not in sessions and h not in domain_owner.values())
                        sessions[new_handle] = 'legacy:applet_proxy'
                        struct.pack_into('<II', response, 8, 0x20, new_handle)
                    if is_open:
                        new_handle = next(h for h in range(0x103, 0x200) if h not in sessions and h not in domain_owner.values())
                        sessions[new_handle] = requested.decode('ascii')
                        struct.pack_into('<II', response, 8, 0x20, new_handle)
                    if is_proxy or (is_proxy_service and not is_legacy_proxy_service) or (not is_legacy and (is_fs_open or is_file_open or is_memory_file_open or is_directory_open)) or is_applet_create or is_storage_create or is_storage_open or is_applet_pop or is_save_reader_open or is_save_open:
                        next_id = max((obj for session, obj in domain_objects if session == owner), default=0) + 1
                        domain_objects[(owner, next_id)] = ('savecard' if is_save_open else 'save_reader' if is_save_reader_open else 'applet_storage' if is_storage_create or is_applet_pop else 'storage_accessor' if is_storage_open else 'player_select' if is_applet_create else 'directory' if is_directory_open else 'memory_file' if is_memory_file_open else 'self_file' if is_file_open else 'sdcard' if is_fs_open else
                                                           'applet_proxy' if is_proxy else proxy_services[command])
                        if is_save_open:
                            domain_saves[(owner, next_id)] = virtual_saves[attr]
                            domain_save_attrs[(owner, next_id)] = attr
                        if is_save_reader_open:
                            space = request[cmif + 16]
                            save_readers[(owner, next_id)] = {'entries': [(k, v) for k, v in virtual_saves.items() if v['space'] == space], 'position': 0}
                        if is_applet_create:
                            applet_objects[(owner, next_id)] = {'inputs': [], 'event': None}
                        if is_storage_create:
                            applet_objects[(owner, next_id)] = bytearray(storage_size)
                        if is_applet_pop:
                            applet_objects[(owner, next_id)] = output_storage
                        if is_storage_open:
                            applet_objects[(owner, next_id)] = applet_objects[(owner, object_id)]
                        if is_directory_open:
                            domain_directories[(owner, next_id)] = directory
                        if is_memory_file_open:
                            domain_files[(owner, next_id)] = (path, mode, fs_files)
                        struct.pack_into('<I', response, 0x10, 1)  # one domain object
                        struct.pack_into('<4sIII', response, 0x20, b'SFCO', 0, 0, 0)
                        struct.pack_into('<I', response, 0x30, next_id)
                    elif (is_aruid or is_event or is_mode or is_notify or is_launch_info or is_fs_init or is_fs_path or is_file_read or is_file_size or is_memory_file_op or is_directory_op or is_applet_state_event or is_storage_io or is_applet_push or is_applet_start or is_applet_result or is_save_reader_read or is_save_create or is_save_commit or is_applet_message) and not is_legacy:
                        struct.pack_into('<4sIII', response, 0x20, b'SFCO', 0, fs_result if is_fs_path else 0, 0)
                        if is_save_reader_read:
                            struct.pack_into('<q', response, 0x30, save_read_count)
                        elif is_storage_io and command == 0:
                            struct.pack_into('<Q', response, 0x30, storage_value)
                        elif is_file_read or is_file_size or is_memory_file_op or is_directory_op or (is_fs_path and command == 7):
                            struct.pack_into('<Q', response, 0x30, file_value)
                        elif is_aruid or (is_mode and command == 9):
                            struct.pack_into('<Q', response, 0x30, 1)
                    else:
                        struct.pack_into('<4sIII', response, 0x10, b'SFCO', 0, 0, 0)
                    if is_applet_message:
                        message_offset = 0x20 if is_legacy else 0x30
                        if applet_messages:
                            struct.pack_into('<I', response, message_offset, applet_messages.pop(0))
                            for event_handle in applet_message_events:
                                if event_handle in events:
                                    events[event_handle] = bool(applet_messages)
                        else:
                            struct.pack_into('<I', response, message_offset - 8, 0x680)
                    if is_legacy:
                        if is_aruid or (is_mode and command == 9):
                            struct.pack_into('<Q', response, 0x20, 1)
                        elif is_fs_path and command == 7:
                            struct.pack_into('<Q', response, 0x20, file_value)
                        elif is_file_read or is_file_size or is_memory_file_op or is_directory_op:
                            struct.pack_into('<Q', response, 0x20, file_value)
                        if is_fs_path and fs_result:
                            struct.pack_into('<I', response, 0x18, fs_result)
                    if is_clock:
                        struct.pack_into('<Q', response, 0x20, int(time.time()))
                    if is_font and command in (1, 2, 3):
                        font_offset, font_size = font_records[font_type]
                        struct.pack_into('<I', response, 0x20, 1 if command == 1 else font_size if command == 2 else font_offset)
                    if is_font and command == 5:
                        struct.pack_into('<B3xI', response, 0x20, 1, len(font_records))
                    if is_nv_open:
                        struct.pack_into('<II', response, 0x20, nv_fd, 0)
                    if is_display_id:
                        vi_state['display'] = True
                    if is_display_id or is_open_layer:
                        struct.pack_into('<Q', response, 0x20, 1 if is_display_id else 28)
                    if is_domain_convert:
                        domain_objects[(handle, 1)] = sessions[handle]
                        struct.pack_into('<I', response, 0x20, 1)
                    if is_query:
                        struct.pack_into('<H', response, 0x20, 0 if sessions.get(handle) == 'nvdrv:a' else 0x500)
                    if is_timezone:
                        timestamp = struct.unpack_from('<q', request, cmif + 16)[0]
                        try:
                            dt = datetime.datetime.fromtimestamp(timestamp, datetime.timezone.utc)
                            struct.pack_into('<H6BII8sIi', response, 0x20, dt.year, dt.month, dt.day,
                                             dt.hour, dt.minute, dt.second, 0, (dt.weekday() + 1) % 7,
                                             dt.timetuple().tm_yday - 1, b'UTC', 0, 0)
                        except (ValueError, OverflowError, OSError):
                            result['reason'] = f'unsupported timezone timestamp {timestamp}'
                            cpu.emu_stop()
                            return
                    cpu.mem_write(tls_address, bytes(response))
                    cpu.reg_write(UC_ARM64_REG_X0, 0)
                    return
                if sessions.get(handle) == 'nvdrv:a' and msg_type == 4 and command == 11:
                    try:
                        h0, h1 = struct.unpack_from('<II', request)
                        if (h1 & 0x80000000 or ((h0 >> 16) & 15, (h0 >> 20) & 15, (h0 >> 24) & 15) != (2, 2, 1)):
                            raise ValueError('unsupported GPU submission descriptors')
                        gpu_fd, ioctl = struct.unpack_from('<II', request, cmif + 16)
                        group = domain_owner.get(handle, handle)
                        channel = gpu_channels.get(gpu_fd)
                        if (ioctl != 0xc018481b or channel is None or channel.owner != group
                                or channel.address_space is None or channel.gpfifo_capacity is None):
                            raise ValueError('invalid GPU submission channel')
                        def submit_desc(pos):
                            size, low, desc = struct.unpack_from('<III', request, pos)
                            addr = low | (((desc >> 28) & 15) << 32) | (((desc >> 2) & 0x3fffff) << 36)
                            return addr, size | (((desc >> 24) & 15) << 32)
                        args_addr, args_size = submit_desc(24)
                        entries_addr, entries_size = submit_desc(36)
                        out_addr, out_size = submit_desc(48)
                        if (args_size != 24 or out_size != 24 or entries_size > 32768
                                or not guest_range_has_permission(cpu, args_addr, args_size, 1)
                                or not guest_range_has_permission(cpu, entries_addr, entries_size, 1)
                                or not writable_guest_range(cpu, out_addr, out_size)):
                            raise ValueError('invalid GPU submission buffers')
                        arguments = bytes(cpu.mem_read(args_addr, args_size))
                        entry_snapshot = bytes(cpu.mem_read(entries_addr, entries_size))
                        def read_gpu_raw(address, size):
                            _, guest = channel.address_space.resolve(address, size)
                            if not guest_range_has_permission(cpu, guest, size, 1):
                                raise ValueError('GPU backing is not readable')
                            return bytes(cpu.mem_read(guest, size))
                        def read_gpu(address, size):
                            if channel.address_space.kind_for_range(address, size) != 0:
                                raise ValueError('GPU linear read from tiled mapping is unsupported')
                            return read_gpu_raw(address, size)
                        fields, decoded = inspect_submission(arguments, entry_snapshot, channel.gpfifo_capacity, read_gpu)
                        first = next(((va, *method) for va, _, _, methods in decoded for method in methods), None)
                        count = sum(len(methods) for _, _, _, methods in decoded)
                        if os.environ.get('SOJU_SWITCH_TRACE_GPU_VALUES'):
                            sequence = [(subchannel, method, value) for _, _, _, methods in decoded
                                        for subchannel, method, value in methods]
                            print(f'soju-switch-gpu: method_sequence={sequence!r}', flush=True)
                        def check_gpu_write(address, size):
                            try:
                                _, guest = channel.address_space.resolve(address, size)
                                return channel.address_space.kind_for_range(address, size) == 0 and writable_guest_range(cpu, guest, size)
                            except ValueError:
                                return False
                        def check_gpu_tiled_write(address, size):
                            try:
                                _, guest = channel.address_space.resolve(address, size)
                                return channel.address_space.kind_for_range(address, size) == 0xfe and writable_guest_range(cpu, guest, size)
                            except ValueError:
                                return False
                        def check_gpu_read(address, size):
                            try:
                                _, guest = channel.address_space.resolve(address, size)
                                return channel.address_space.kind_for_range(address, size) == 0 and guest_range_has_permission(cpu, guest, size, 1)
                            except ValueError:
                                return False
                        try:
                            staged = gpu_execution_states[gpu_fd].stage(decoded, check_gpu_write, check_gpu_tiled_write, check_gpu_read)
                        except UnsupportedGpuMethod as exc:
                            raise ValueError(f'GPU execution not implemented: decoded {count} methods; {exc}') from exc
                        # A CPU-only commit may not overwrite the request, reply, TLS,
                        # or command bytes that were snapshotted for this submission.
                        protected = [(args_addr, args_size), (entries_addr, entries_size),
                                     (out_addr, out_size), (tls_address, 0x100)]
                        for low, high in struct.iter_unpack('<II', entry_snapshot):
                            entry = decode_entry(low, high)
                            if entry is not None:
                                va, words, _, _ = entry
                                _, command_guest = channel.address_space.resolve(va, words * 4)
                                protected.append((command_guest, words * 4))
                        if out_addr < tls_address + 0x100 and tls_address < out_addr + out_size:
                            raise ValueError('GPU reply overlaps IPC state')
                        for action in staged.pending_actions:
                            if action[0] in ('write', 'tiled_write'):
                                write_va, data = action[1:]
                                _, write_guest = channel.address_space.resolve(write_va, len(data))
                                if any(write_guest < start + size and start < write_guest + len(data)
                                       for start, size in protected):
                                    raise ValueError('GPU write overlaps submission control data')
                        for action in staged.pending_actions:
                            if action[0] == 'pitch_to_tiled':
                                for _, write_va, size in action[1]:
                                    _, write_guest = channel.address_space.resolve(write_va, size)
                                    if any(write_guest < start + length and start < write_guest + size
                                           for start, length in protected):
                                        raise ValueError('GPU copy overlaps submission control data')
                        if not writable_guest_range(cpu, tls_address, 0x100):
                            raise ValueError('GPU IPC reply is not writable')
                        original_output = bytes(cpu.mem_read(out_addr, out_size))
                        original_tls = bytes(cpu.mem_read(tls_address, 0x100))
                        response = bytearray(0x100)
                        struct.pack_into('<II', response, 0, 0, 12)
                        struct.pack_into('<4sIII', response, 0x10, b'SFCO', 0, 0, 0)
                        def write_gpu(address, data):
                            _, guest = channel.address_space.resolve(address, len(data))
                            cpu.mem_write(guest, data)
                        def publish_reply(fence_id, target):
                            output = bytearray(arguments)
                            struct.pack_into('<II', output, 16, fence_id, target)
                            cpu.mem_write(out_addr, bytes(output))
                            cpu.mem_write(tls_address, bytes(response))
                        def restore_reply():
                            failures = []
                            for address, data in ((out_addr, original_output), (tls_address, original_tls)):
                                try:
                                    cpu.mem_write(address, data)
                                except UcError as exc:
                                    failures.append(exc)
                            if failures:
                                raise RuntimeError('GPU reply rollback failed')
                        commit_cpu_submission(channel, staged, fields[1], fields[2], fields[3],
                                              read_gpu_raw, write_gpu, check_gpu_write, publish_reply, restore_reply,
                                              check_gpu_tiled_write, check_gpu_read,
                                              lambda address, size: channel.address_space.resolve(address, size)[1])
                        gpu_execution_states[gpu_fd] = staged
                        cpu.reg_write(UC_ARM64_REG_X0, 0)
                        if os.environ.get('SOJU_SWITCH_TRACE_GPU'):
                            print(f'soju-switch-gpu: CPU-only submission committed {count} methods, fence {channel.syncpoint}:{channel.completed}', flush=True)
                        return
                    except (ValueError, RuntimeError, UcError, struct.error) as exc:
                        result['reason'] = f'GPU submission rejected: {exc}'
                    cpu.emu_stop()
                    return
                result["reason"] = (f"unimplemented IPC session=0x{handle:x} service={sessions.get(handle)!r} object={object_kind!r} "
                                    f"type={msg_type} cmd={command} tls={request[:144].hex()}")
                cpu.emu_stop()
                return
            if svc == 6:  # QueryMemory for mapped diagnostic regions.
                output = cpu.reg_read(UC_ARM64_REG_X0)
                query = cpu.reg_read(UC_ARM64_REG_X2)
                for start, end, permissions in cpu.mem_regions():
                    if start <= query <= end:
                        if STACK <= query < STACK + STACK_SIZE or any(dest <= query < dest + size for dest, _, size in stack_aliases):
                            memtype = 0x0b  # Stack
                        elif 0x400000000 <= query < 0x400000000 + heap_size:
                            memtype = 5  # Normal heap
                        elif BASE <= query < BASE + mapped:
                            memtype = 4  # CodeData (simplified)
                        elif any(address <= query < address + size for (_, address), size in shared_mappings.items()):
                            memtype = 6  # SharedMemory
                        elif any(t['tls'] <= query < t['tls'] + 0x1000 for t in threads.values()):
                            memtype = 0x0c  # ThreadLocal
                        else:
                            break
                        def page_attrs(page):
                            return (0x10 if page in locked_pages else 0) | (8 if page in uncached_pages else 0)
                        page = query & ~0xfff
                        attrs = page_attrs(page)
                        if any(start <= p <= end for p in locked_pages | uncached_pages):
                            attr_start = page
                            attr_end = page + 0x1000
                            while attr_start > start and page_attrs(attr_start - 0x1000) == attrs:
                                attr_start -= 0x1000
                            while attr_end <= end and page_attrs(attr_end) == attrs:
                                attr_end += 0x1000
                            start, end = max(start, attr_start), min(end, attr_end - 1)
                        try:
                            cpu.mem_write(output, struct.pack('<QQIIIIII', start, end - start + 1,
                                                              memtype, attrs, permissions, 0, 0, 0))
                            cpu.reg_write(UC_ARM64_REG_X0, 0)
                            cpu.reg_write(UC_ARM64_REG_X1, 0)
                            return
                        except UcError:
                            break
                # Querying a free address returns the entire surrounding free interval.
                regions = list(cpu.mem_regions())
                if 0 <= query < 0x8000000000 and not any(start <= query <= end for start, end, _ in regions):
                    before = max((end + 1 for start, end, _ in regions if end < query), default=0)
                    after = min((start for start, end, _ in regions if start > query), default=0x8000000000)
                    try:
                        cpu.mem_write(output, struct.pack('<QQIIIIII', before, after - before, 0, 0, 0, 0, 0, 0))
                        cpu.reg_write(UC_ARM64_REG_X0, 0)
                        cpu.reg_write(UC_ARM64_REG_X1, 0)
                        return
                    except UcError:
                        pass
                result["reason"] = f"unsupported QueryMemory address=0x{query:x}"
                cpu.emu_stop()
                return
            if svc == 0x29:  # GetInfo: deterministic virtual address regions.
                info_id = cpu.reg_read(UC_ARM64_REG_X1)
                subtype = cpu.reg_read(UC_ARM64_REG_X3)
                info = {0: 1, 1: 0xffffffffffffffff, 2: 0x500000000, 3: 0x40000000,
                        4: 0x400000000, 5: 0x10000000,
                        6: 0x10000000, 7: mapped + STACK_SIZE,
                        12: BASE, 13: 0x600000000,
                        14: STACK, 15: 0x10000000, 18: program_id, 20: 0x300010000,
                        21: 0x10000000, 22: mapped + STACK_SIZE + heap_size, 23: 0,
                        24: max(0, 16 - len(threads)), 28: 0}
                if (subtype == 0 and info_id in info) or (info_id == 11 and subtype < 4):
                    cpu.reg_write(UC_ARM64_REG_X0, 0)
                    cpu.reg_write(UC_ARM64_REG_X1, secrets.randbits(64) if info_id == 11 else info[info_id])
                    return
                if info_id == 19:  # InitialProcessIdRange is unavailable on simulated HOS 17.
                    cpu.reg_write(UC_ARM64_REG_X0, 0xf001)
                    return
                result["reason"] = f"unsupported GetInfo id={info_id} subtype={subtype}"
                cpu.emu_stop()
                return
            if svc == 3:  # SetMemoryAttribute for supported guest memory attributes.
                address = cpu.reg_read(UC_ARM64_REG_X0)
                size = cpu.reg_read(UC_ARM64_REG_X1)
                mask = cpu.reg_read(UC_ARM64_REG_X2)
                value = cpu.reg_read(UC_ARM64_REG_X3)
                data_offset = struct.unpack_from("<I", image, 0x30)[0]
                if (address % 0x1000 == 0 and size > 0 and size % 0x1000 == 0
                        and BASE + data_offset <= address < BASE + mapped
                        and size <= BASE + mapped - address and mask == 0x10 and value == 0x10):
                    locked_pages.update(range(address, address + size, 0x1000))
                    cpu.reg_write(UC_ARM64_REG_X0, 0)
                    return
                if (address % 0x1000 == 0 and size > 0 and size % 0x1000 == 0
                        and 0x400000000 <= address < 0x400000000 + heap_size
                        and size <= 0x400000000 + heap_size - address
                        and mask == 8 and value in (0, 8)
                        and guest_range_has_permission(cpu, address, size, 3)):
                    pages = range(address, address + size, 0x1000)
                    if value:
                        uncached_pages.update(pages)
                    else:
                        uncached_pages.difference_update(pages)
                    cpu.reg_write(UC_ARM64_REG_X0, 0)
                    return
                result["reason"] = f"unsupported SetMemoryAttribute at offset 0x{pc - BASE - 4:x} address=0x{address:x} size=0x{size:x} mask=0x{mask:x} value=0x{value:x}"
                cpu.emu_stop()
                return
            result["reason"] = (f"unimplemented SVC 0x{svc:x} at offset 0x{pc - BASE - 4:x}; "
                                f"x0=0x{cpu.reg_read(UC_ARM64_REG_X0):x}, "
                                f"x1=0x{cpu.reg_read(UC_ARM64_REG_X1):x}, "
                                f"x2=0x{cpu.reg_read(UC_ARM64_REG_X2):x}")
        else:
            result["reason"] = f"interrupt {kind} at pc=0x{pc:x}"
        cpu.emu_stop()

    def on_invalid(cpu, access, address, size, value, unused):
        result["reason"] = f"invalid memory access at 0x{address:x}, pc=0x{cpu.reg_read(UC_ARM64_REG_PC):x}"
        return False

    uc.hook_add(UC_HOOK_INTR, on_interrupt)
    uc.hook_add(UC_HOOK_MEM_INVALID, on_invalid)
    try:
        start = BASE
        remaining = max_steps
        idle_since = None
        while True:
            quantum = 100000 if window else min(remaining, 100000)
            uc.emu_start(start, BASE + len(image), count=quantum)
            if result['reason'] != 'instruction budget or end address reached':
                break
            if window:
                if window.poll() and not request_guest_exit():
                    result['reason'] = f'window closed after {frame_count} frames (no applet exit event)'
                    break
                if exit_requested and time.monotonic() >= exit_deadline:
                    result['reason'] = f'window closed: guest did not exit within 30 seconds; thread=0x{current_thread:x} pc=0x{uc.reg_read(UC_ARM64_REG_PC):x} lr=0x{uc.reg_read(arm64_const.UC_ARM64_REG_X30):x} states={[(hex(h), t["state"], t.get("last_sleep")) for h, t in threads.items()]}; save may be incomplete'
                    break
                if hid_address is not None:
                    update_hid(uc, window.controller_mask())
            if not window:
                remaining -= quantum
                if remaining <= 0:
                    break
            if not scheduling:
                start = uc.reg_read(UC_ARM64_REG_PC)
                if start == BASE + len(image):
                    break
                continue
            current = threads[current_thread]
            current['context'] = uc.context_save()
            if current['state'] == 'running':
                current['state'] = 'runnable'
            while True:
                now = time.monotonic_ns()
                pump_audio()
                for t in threads.values():
                    if t['state'] == 'sleeping' and now >= t['deadline']:
                        t['state'] = 'runnable'
                    elif t['state'] == 'cond_wait' and t['deadline'] is not None and now >= t['deadline']:
                        t['pending'] = (0xea01, 0)
                        t['state'] = 'runnable'
                    elif t['state'] == 'waiting':
                        ready = next((i for i, h in enumerate(t['wait_handles'])
                                      if (h in events and events[h]) or (h in threads and threads[h]['state'] == 'exited')), None)
                        if ready is not None:
                            t['pending'] = (0, ready)
                            t['state'] = 'runnable'
                        elif t['deadline'] is not None and now >= t['deadline']:
                            t['pending'] = (0xea01, 0)
                            t['state'] = 'runnable'
                handles = list(threads)
                previous = handles.index(current_thread)
                ready = [h for h in handles[previous + 1:] + handles[:previous + 1]
                         if threads[h]['state'] == 'runnable']
                if ready:
                    idle_since = None
                    break
                if not any(t['state'] in ('sleeping', 'waiting', 'cond_wait', 'mutex_wait') for t in threads.values()):
                    result['reason'] = 'all started guest threads exited'
                    break
                if idle_since is None:
                    idle_since = time.monotonic()
                if time.monotonic() - idle_since > 2:
                    result['reason'] = 'guest threads blocked: diagnostic idle limit of 2 seconds reached'
                    break
                if window:
                    if window.poll() and not request_guest_exit():
                        result['reason'] = f'window closed after {frame_count} frames (no applet exit event)'
                        break
                    if exit_requested and time.monotonic() >= exit_deadline:
                        result['reason'] = f'window closed: guest did not exit within 30 seconds; thread=0x{current_thread:x} pc=0x{uc.reg_read(UC_ARM64_REG_PC):x} lr=0x{uc.reg_read(arm64_const.UC_ARM64_REG_X30):x} states={[(hex(h), t["state"], t.get("last_sleep")) for h, t in threads.items()]}; save may be incomplete'
                        break
                    if hid_address is not None:
                        update_hid(uc, window.controller_mask())
                time.sleep(0.001)
            if result['reason'] != 'instruction budget or end address reached':
                break
            current_thread = ready[0]
            selected = threads[current_thread]
            if selected['context'] is None:
                for number in range(31):
                    uc.reg_write(getattr(arm64_const, f'UC_ARM64_REG_X{number}'), 0)
                uc.reg_write(UC_ARM64_REG_X0, selected['arg'])
                uc.reg_write(UC_ARM64_REG_SP, selected['stack'])
                uc.reg_write(UC_ARM64_REG_PC, selected['entry'])
                uc.reg_write(UC_ARM64_REG_TPIDRRO_EL0, selected['tls'])
                selected['context'] = uc.context_save()
            else:
                uc.context_restore(selected['context'])
            if 'pending' in selected:
                rc, index = selected.pop('pending')
                uc.reg_write(UC_ARM64_REG_X0, rc)
                uc.reg_write(UC_ARM64_REG_X1, index)
            selected['state'] = 'running'
            start = uc.reg_read(UC_ARM64_REG_PC)
    except UcError as exc:
        if result["reason"] == "instruction budget or end address reached":
            result["reason"] = f"CPU error: {exc}"
    finally:
        save_store.close()
        for audio in audio_outputs.values():
            if audio['device']:
                audio['device'].close()
        if window:
            window.close()
    print("soju-switch-trace:", result["reason"])
    return 0 if (window and result['reason'] in ('guest exited normally', 'guest exited')) or (window and result['reason'].startswith('window closed after ')) or result['reason'].startswith('captured frame ') else 1

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("nro", type=Path)
    parser.add_argument("--max-steps", type=int, default=1_000_000)
    parser.add_argument("--capture", type=Path, help="save the final submitted frame as a PNG")
    parser.add_argument("--capture-first", type=Path, help="save the first frame as a PNG")
    parser.add_argument("--moves", default='', help="comma-separated test inputs: left,up,right,down,b")
    parser.add_argument("--play", action='store_true', help="open experimental SDL2 window (arrows, B, Escape)")
    args = parser.parse_args()
    moves = tuple(args.moves.split(',')) if args.moves else ()
    if any(move not in ('left', 'up', 'right', 'down', 'b') for move in moves) or len(moves) > 20:
        parser.error('invalid moves; use up to 20 comma-separated left,up,right,down,b inputs')
    if not 1 <= args.max_steps <= 10_000_000_000:
        parser.error("--max-steps must be between 1 and 10000000000")
    try:
        return trace(args.nro, args.max_steps, args.capture, moves, args.capture_first, args.play)
    except StopIteration:
        print('soju-switch-trace: emulated handle limit reached', file=sys.stderr)
        return 1
    except (OSError, ValueError, UcError, RuntimeError, struct.error) as exc:
        print(f"soju-switch-trace: {exc}", file=sys.stderr)
        return 1

if __name__ == "__main__":
    sys.exit(main())
