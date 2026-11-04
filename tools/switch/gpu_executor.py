"""Transactional subset of virtual GPU command execution.

Only object bindings and MME uploads are implemented. Unknown state, draw,
memory and synchronization methods fail closed. No submission or fence is
completed by this module.
"""
import copy
from collections import deque
import struct
from gpu_mme import MacroUpload
from gpu_mme_interpreter import run_macro
from gpu_surface import plan_uniform_clear, plan_pitch_to_tiled

SUPPORTED_CLASSES = frozenset((0xa140, 0xb0b5, 0x902d, 0xb197, 0xb1c0))
# Explicitly checked state fields; execution effects are handled separately.
B197_STATE_MASKS = {
    0x37c: 1, 0x74c: 0xff, 0xfac: 1, 0xfdc: 1, 0x133c: 1, 0x135c: 1,
    0x1534: 1, 0x153c: 0x11, 0x13ac: 0x11, 0x151c: 1, 0x15e4: 1,
    0x164c: 0x1000, 0x1610: 0x3f, 0x1140: 0x10, 0x10f8: 0x1111,
    0x20c: 1, 0x2608: 0x1f, 0x30c: 1, 0x300: 3, 0x2d0: 0x3fffff,
    0x10cc: 0x00ff00ff, 0x10e0: 0x00ff00ff, 0x10e4: 0x00ff00ff,
    0x10ec: 0x00ff00ff, 0x10f0: 0x00ff00ff,
    0x1608: 0xff, 0x160c: 0xffffffff, 0xf84: 0xff, 0xf88: 0xffffffff,
    0x790: 0xff, 0x794: 0xffffffff, 0x798: 0x3f, 0x79c: 0xffffffff,
    0x7a0: 0x3ffffff, 0x77c: 0xffffffff, 0x1574: 0xff, 0x1578: 0xffffffff,
    0x157c: 0x3fffff, 0x155c: 0xff, 0x1560: 0xffffffff, 0x1564: 0xfffff,
    0x1590: 0x3f, 0x15cc: 0x1ffff,
    0xff4: 0xffffffff, 0xff8: 0xffffffff, 0x800: 0xff, 0x804: 0xffffffff,
    0x808: 0x0fffffff, 0x80c: 0x1ffff, 0x818: 0x0fffffff, 0x81c: 0xffffffff, 0x820: 0xffff,
    0xfe0: 0xff, 0xfe4: 0xffffffff, 0xff0: 0xffffffff, 0x1538: 1,
    0x1228: 0x0fffffff, 0x122c: 0x1ffff, 0x1230: 0x1ffff, 0x179c: 0xffff, 0x754: 0x3f,
    0x11e0: 0xffffffff, 0x11e4: 0xffffffff, 0x11e8: 0xffffffff, 0x11ec: 0xffffffff,
    0xd80: 0xffffffff, 0xd84: 0xffffffff, 0xd88: 0xffffffff, 0xd8c: 0xffffffff,
    0x19c4: 1, 0x12e4: 1, 0xf90: 1, 0x1a00: 0x1111, 0x12cc: 1, 0x19bc: 1,
    0x1380: 1, 0x12ec: 1, 0x1684: 1, 0x1688: 1, 0x2600: 1, 0x13a8: 0x33333333,
    0x1570: 1, 0x13b0: 0xffffffff, 0x166c: 1, 0x1910: 0xff1, 0x1520: 1,
    0x1658: 1, 0xdb4: 1, 0x1918: 1, 0x168c: 1, 0xdc0: 1, 0xdc4: 1, 0xdc8: 1,
    0xd7c: 1, 0x1924: 1, 0x1148: 1,
    0x324: 0xffffffff, 0x328: 0xffffffff, 0x32c: 0xffffffff,
    0x330: 0xffffffff, 0x334: 0xffffffff, 0x338: 0xffffffff,
}


B1C0_STATE_MASKS = {
    0x790: 0xff, 0x794: 0xffffffff, 0x2e4: 0xff, 0x2e8: 0xffffffff, 0x2ec: 0x1ff,
    0x2f0: 0xff, 0x2f4: 0xffffffff, 0x2f8: 0x1ff, 0x77c: 0xffffffff,
    0x214: 0xffffffff, 0x1608: 0xff, 0x160c: 0xffffffff, 0x310: 0xffff,
    0x1574: 0xff, 0x1578: 0xffffffff, 0x157c: 0x3fffff,
    0x155c: 0xff, 0x1560: 0xffffffff, 0x1564: 0xfffff, 0x2608: 7,
    0x188: 0xff, 0x18c: 0xffffffff, 0x180: 0xffffffff, 0x184: 0xffffffff,
}


class UnsupportedGpuMethod(ValueError):
    def __init__(self, message, partial=None):
        super().__init__(message)
        self.partial = partial


def normalize_macro_arguments(decoded, initial_bindings):
    """Collect the observed four-continuation vertex macro without reordering."""
    commands = [(address, not_main, no_prefetch, sub, method, value)
                for address, not_main, no_prefetch, methods in decoded for sub, method, value in methods]
    bindings = dict(initial_bindings)
    output, arguments = [], deque()
    index = 0
    while index < len(commands):
        address, not_main, no_prefetch, subchannel, method, value = commands[index]
        if method == 0:
            bindings[subchannel] = value & 0xffff
        output.append((address, not_main, no_prefetch, [(subchannel, method, value)]))
        if bindings.get(subchannel) == 0xb197 and method == 0x3808:
            following = commands[index + 1:index + 5]
            if len(following) != 4 or any(sub != subchannel or m != 0x380c for _, _, _, sub, m, _ in following):
                raise UnsupportedGpuMethod('vertex macro requires four continuation parameters')
            arguments.append(tuple(word for _, _, _, _, _, word in following))
            index += 4
        index += 1
    return output, arguments


class GpuExecutionState:
    def __init__(self):
        self.bindings = {}
        self.registers = {}
        self.compute_ref_counters = {}
        self.constant_bindings = {}
        self.pending_writes = []
        self.pending_actions = []
        self.inflight_work = False
        self.inline_uploads = {}
        self.mme = MacroUpload()

    def stage(self, decoded, check_gpu_write=None, check_gpu_tiled_write=None, check_gpu_read=None):
        """Apply supported commands to a copy; the caller commits only on success."""
        staged = copy.deepcopy(self)
        decoded, macro_parameters = normalize_macro_arguments(decoded, staged.bindings)
        for address, _, _, methods in decoded:
            for subchannel, method, value in methods:
                if not 0 <= subchannel <= 7 or not 0 <= value <= 0xffffffff:
                    raise ValueError('invalid GPU method')
                if method == 0:
                    class_id = value & 0xffff
                    if value != class_id or class_id not in SUPPORTED_CLASSES:
                        raise UnsupportedGpuMethod(f'unsupported GPU object 0x{value:x} at 0x{address:x}')
                    if subchannel in staged.bindings and staged.bindings[subchannel] != class_id:
                        raise UnsupportedGpuMethod('unsupported GPU subchannel class rebind')
                    staged.bindings[subchannel] = class_id
                    if class_id == 0xb197:
                        # Explicit Soju virtual reset values for this limited profile.
                        for reg, initial in ((0xdac, 0x1b02), (0xdb0, 0x1b02), (0x20c0, 0), (0x2100, 0)):
                            staged.registers.setdefault((subchannel, reg), initial)
                elif staged.bindings.get(subchannel) == 0x902d and method in (0x104, 0x108, 0x260, 0x26c, 0x2ac, 0x290, 0x29c, 0x884, 0x888):
                    valid = ((method in (0x260, 0x290, 0x29c, 0x888) and value in (0, 1))
                             or (method == 0x104 and value <= 0x1ffffff)
                             or (method == 0x108)
                             or (method == 0x26c and value <= 4)
                             or (method == 0x2ac and value == 3)
                             or (method == 0x884 and value <= 0x3ff))
                    if not valid:
                        raise UnsupportedGpuMethod(f'unsupported 2D register value 0x{value:x} for 0x{method:x}')
                    staged.registers[(subchannel, method)] = value
                elif staged.bindings.get(subchannel) == 0xb0b5 and method in (0x708, 0x70c, 0x710, 0x714, 0x718, 0x71c, 0x720, 0x400, 0x404, 0x408, 0x40c, 0x410, 0x414, 0x418, 0x41c):
                    if ((method == 0x708 and value != 0x03303210)
                            or (method == 0x70c and value != 0x1040)
                            or (method in (0x400, 0x408) and value > 0xff)
                            or (method == 0x718 and value != 1)
                            or (method in (0x71c, 0x720) and value != 0)):
                        raise UnsupportedGpuMethod('unsupported GPU copy configuration')
                    staged.registers[(subchannel, method)] = value
                elif staged.bindings.get(subchannel) == 0xb0b5 and method == 0x300:
                    regs = {reg: staged.registers.get((subchannel, reg)) for reg in (
                        0x708, 0x70c, 0x710, 0x714, 0x718, 0x71c, 0x720,
                        0x400, 0x404, 0x408, 0x40c, 0x410, 0x414, 0x418, 0x41c)}
                    if (value != 0x686 or any(v is None for v in regs.values())
                            or (regs[0x708], regs[0x70c], regs[0x710], regs[0x714], regs[0x718], regs[0x71c], regs[0x720]) != (0x03303210, 0x1040, 301, 76, 1, 0, 0)
                            or (regs[0x410], regs[0x414], regs[0x418], regs[0x41c]) != (1204, 1216, 301, 76)):
                        raise UnsupportedGpuMethod('unsupported pitch-to-tiled copy configuration')
                    source = (regs[0x400] << 32) | regs[0x404]
                    target = (regs[0x408] << 32) | regs[0x40c]
                    if source % 4 or target % 0x10000:
                        raise UnsupportedGpuMethod('unsupported GPU copy alignment')
                    copies = plan_pitch_to_tiled(source, target, 301, 76, 1204, 4)
                    if (check_gpu_read is None or check_gpu_tiled_write is None
                            or not all(check_gpu_read(source + row * 1204, 1204) for row in range(76))
                            or not all(check_gpu_tiled_write(dst, size) for _, dst, size in copies)):
                        raise UnsupportedGpuMethod('GPU copy source or target is unavailable')
                    staged.pending_actions.append(('pitch_to_tiled', copies))
                elif staged.bindings.get(subchannel) in (0xb1c0, 0xa140) and method == 0x1b0:
                    keys = [(subchannel, reg) for reg in (0x188, 0x18c, 0x180, 0x184)]
                    if value not in (0x41, 0x1001) or subchannel in staged.inline_uploads or any(key not in staged.registers for key in keys):
                        raise UnsupportedGpuMethod('unsupported compute inline launch')
                    high, low, length, lines = (staged.registers[key] for key in keys)
                    target = (high << 32) | low
                    if (lines != 1 or length == 0 or length % 4 or length > 8 * 1024 * 1024
                            or target % 4 or target + length > 1 << 40
                            or check_gpu_write is None or not check_gpu_write(target, length)):
                        raise UnsupportedGpuMethod('invalid compute inline destination')
                    staged.inline_uploads[subchannel] = (target, length, value == 0x1001)
                elif staged.bindings.get(subchannel) in (0xb1c0, 0xa140) and method == 0x1b4:
                    if subchannel not in staged.inline_uploads:
                        raise UnsupportedGpuMethod('compute inline data without launch')
                    target, remaining, needs_barrier = staged.inline_uploads[subchannel]
                    write = (target, struct.pack('<I', value))
                    staged.pending_writes.append(write)
                    staged.pending_actions.append(('write', *write))
                    if remaining == 4:
                        del staged.inline_uploads[subchannel]
                        if needs_barrier:
                            staged.pending_actions.append(('memory_barrier', subchannel))
                    else:
                        staged.inline_uploads[subchannel] = (target + 4, remaining - 4, needs_barrier)
                elif staged.bindings.get(subchannel) == 0xb1c0 and method == 0x1698:
                    if value != 0x1000:
                        raise UnsupportedGpuMethod('unsupported compute cache invalidation')
                    staged.pending_actions.append(('invalidate_constant_cache', subchannel))
                elif staged.bindings.get(subchannel) == 0xb1c0 and method == 0x110:
                    if value != 0 or staged.inflight_work:
                        raise UnsupportedGpuMethod('compute wait requires in-flight GPU work to complete')
                    staged.pending_actions.append(('wait_idle', subchannel))
                elif staged.bindings.get(subchannel) == 0xa140 and method in (0x180, 0x184, 0x188, 0x18c, 0x190):
                    mask = 0x1ffffff if method == 0x188 else 0xffffffff
                    if value & ~mask:
                        raise UnsupportedGpuMethod('unsupported inline transfer configuration')
                    staged.registers[(subchannel, method)] = value
                elif staged.bindings.get(subchannel) == 0xb1c0 and method in B1C0_STATE_MASKS:
                    if value & ~B1C0_STATE_MASKS[method]:
                        raise UnsupportedGpuMethod(f'unsupported compute state value for 0x{method:x}')
                    staged.registers[(subchannel, method)] = value
                elif staged.bindings.get(subchannel) == 0xb1c0 and method == 0x248:
                    if value & ~0xffff3f:
                        raise UnsupportedGpuMethod('unsupported compute reference counter value')
                    staged.compute_ref_counters[(subchannel, value & 0x3f)] = (value >> 8) & 0xffff
                elif staged.bindings.get(subchannel) == 0xb197 and method in (0x3808, 0x3810, 0x3818, 0x3820, 0x3828, 0x3830):
                    entry = (method - 0x3800) // 8
                    def read_state(reg):
                        key = (subchannel, reg)
                        if key not in staged.registers:
                            raise ValueError(f'MME read from unsupported GPU state 0x{reg:x}')
                        return staged.registers[key]
                    parameters = macro_parameters.popleft() if method == 0x3808 else ()
                    outputs = run_macro(staged.mme, entry, value, read_state, parameters)
                    if any(0x3800 <= reg <= 0x3ffc for reg, _ in outputs):
                        raise UnsupportedGpuMethod('recursive MME method call')
                    generated = [(subchannel, reg, word) for reg, word in outputs]
                    staged = staged.stage([(address, False, False, generated)], check_gpu_write, check_gpu_tiled_write, check_gpu_read)
                elif staged.bindings.get(subchannel) == 0xb197 and (
                        (0x1c04 <= method <= 0x1cf8 and (method - 0x1c04) % 16 in (0, 4))
                        or (0x1f00 <= method <= 0x1f7c and (method - 0x1f00) % 8 in (0, 4))):
                    upper = ((0x1c04 <= method <= 0x1cf4 and (method - 0x1c04) % 16 == 0)
                             or (0x1f00 <= method <= 0x1f78 and (method - 0x1f00) % 8 == 0))
                    if upper and value > 0xff:
                        raise UnsupportedGpuMethod('unsupported vertex stream address')
                    staged.registers[(subchannel, method)] = value
                elif staged.bindings.get(subchannel) == 0xb197 and 0x1360 <= method <= 0x137c and (method - 0x1360) % 4 == 0:
                    if value not in (0, 1):
                        raise UnsupportedGpuMethod('unsupported per-target blend enable')
                    staged.registers[(subchannel, method)] = value
                elif staged.bindings.get(subchannel) == 0xb197 and method in (0xdac, 0xdb0):
                    if value not in (0x1b00, 0x1b01, 0x1b02):
                        raise UnsupportedGpuMethod('unsupported polygon mode')
                    staged.registers[(subchannel, method)] = value
                elif staged.bindings.get(subchannel) == 0xb197 and method in (0x2000, 0x2040, 0x2080, 0x20c0, 0x2100, 0x2140):
                    expected_type = {0x2000: 0, 0x2040: 1, 0x2080: 2, 0x20c0: 3, 0x2100: 4, 0x2140: 5}[method]
                    if value & ~0xf1 or (value >> 4) != expected_type:
                        raise UnsupportedGpuMethod('unsupported shader pipeline state')
                    staged.registers[(subchannel, method)] = value
                elif staged.bindings.get(subchannel) == 0xb197 and 0x2004 <= method <= 0x2150 and (method - 0x2000) % 64 in (4, 12, 16):
                    member = (method - 0x2000) % 64
                    mask = 0xffffffff if member == 4 else 0xff if member == 12 else 7
                    if value & ~mask:
                        raise UnsupportedGpuMethod('unsupported shader program descriptor')
                    staged.registers[(subchannel, method)] = value
                elif staged.bindings.get(subchannel) == 0xb197 and method == 0x2ec:
                    if value & ~0xff1:
                        raise UnsupportedGpuMethod('unsupported raster bounding box state')
                    staged.registers[(subchannel, method)] = value
                elif staged.bindings.get(subchannel) == 0xb197 and method == 0x19d0:
                    regs = {reg: staged.registers.get((subchannel, reg)) for reg in (
                        0x800, 0x804, 0x808, 0x80c, 0x810, 0x814, 0x818, 0x820,
                        0xff4, 0xff8, 0x10f8, 0x121c, 0x15d0, 0x1558, 0x19e0, 0xd80, 0xd84, 0xd88, 0xd8c)}
                    if (value != 0x3c or any(v is None for v in regs.values())
                            or (regs[0x808], regs[0x80c], regs[0x810], regs[0x814], regs[0x818], regs[0x820]) != (1280, 720, 0xd5, 0x40, 1, 0)
                            or (regs[0xff4], regs[0xff8], regs[0x10f8], regs[0x15d0]) != (0x05000000, 0x02d00000, 0, 0)
                            or (regs[0x121c] & 0x7f) != 1 or (regs[0x1558], regs[0x19e0]) != (1, 0)
                            or tuple(regs[r] for r in (0xd80, 0xd84, 0xd88, 0xd8c)) != (0, 0, 0, 0x3f800000)):
                        raise UnsupportedGpuMethod('unsupported color clear configuration')
                    target = (regs[0x800] << 32) | regs[0x804]
                    if target % 0x10000:
                        raise UnsupportedGpuMethod('unsupported color target alignment')
                    writes = plan_uniform_clear(target, 1280, 720, 4, b'\x00\x00\x00\xff')
                    if check_gpu_tiled_write is None or not all(check_gpu_tiled_write(addr, len(data)) for addr, data in writes):
                        raise UnsupportedGpuMethod('color clear target is not writable tiled memory')
                    for write in writes:
                        staged.pending_writes.append(write)
                        staged.pending_actions.append(('tiled_write', *write))
                elif staged.bindings.get(subchannel) == 0xb197 and method == 0x21c:
                    if value != 0x1011 or staged.inflight_work:
                        raise UnsupportedGpuMethod('unsupported shader cache invalidation')
                    staged.pending_actions.append(('invalidate_shader_caches', subchannel))
                elif staged.bindings.get(subchannel) == 0xb197 and method == 0x1234:
                    if value != 0:
                        raise UnsupportedGpuMethod('unsupported sampler binding mode')
                    staged.registers[(subchannel, method)] = value
                elif staged.bindings.get(subchannel) == 0xb197 and method in (0x1144, 0xf80):
                    if value != 0 or staged.inflight_work:
                        raise UnsupportedGpuMethod('unsupported GPU barrier state')
                    action = 'flush_pending_writes' if method == 0x1144 else 'flush_tiled_cache'
                    staged.pending_actions.append((action, subchannel))
                elif staged.bindings.get(subchannel) == 0xb197 and method == 0x2c8:
                    if value & ~0x110fff:
                        raise UnsupportedGpuMethod('unsupported 3D syncpoint increment flags')
                    staged.pending_actions.append(('increment_syncpoint', value & 0xfff, (value >> 20) & 1, bool(value & 0x10000)))
                elif staged.bindings.get(subchannel) == 0xb197 and 0x2410 <= method <= 0x2490 and (method - 0x2410) % 32 == 0:
                    if value & ~0x1f1:
                        raise UnsupportedGpuMethod('invalid constant-buffer binding flags')
                    group = (method - 0x2410) // 32
                    slot = (value >> 4) & 31
                    key = (subchannel, group, slot)
                    if value & 1:
                        keys = [(subchannel, reg) for reg in (0x2380, 0x2384, 0x2388)]
                        if any(k not in staged.registers for k in keys):
                            raise UnsupportedGpuMethod('constant-buffer binding before selector initialization')
                        size, high, low = (staged.registers[k] for k in keys)
                        address = (high << 32) | low
                        if size == 0 or address + size > 1 << 40:
                            raise UnsupportedGpuMethod('invalid constant-buffer binding range')
                        staged.constant_bindings[key] = (address, size)
                    else:
                        staged.constant_bindings.pop(key, None)
                elif staged.bindings.get(subchannel) == 0xb197 and method in (0x2380, 0x2384, 0x2388, 0x238c):
                    masks = {0x2380: 0x1ffff, 0x2384: 0xff, 0x2388: 0xffffffff, 0x238c: 0xffff}
                    if value & ~masks[method]:
                        raise UnsupportedGpuMethod(f'unsupported constant-buffer selector value for 0x{method:x}')
                    staged.registers[(subchannel, method)] = value
                elif staged.bindings.get(subchannel) == 0xb197 and method == 0x2390:
                    keys = [(subchannel, reg) for reg in (0x2380, 0x2384, 0x2388, 0x238c)]
                    if any(key not in staged.registers for key in keys):
                        raise UnsupportedGpuMethod('constant-buffer upload before selector initialization')
                    size, high, low, offset = (staged.registers[key] for key in keys)
                    target = (high << 32) | low
                    if offset % 4 or offset + 4 > size or target + offset + 4 > 1 << 40:
                        raise UnsupportedGpuMethod('constant-buffer upload outside selector bounds')
                    if check_gpu_write is None or not check_gpu_write(target + offset, 4):
                        raise UnsupportedGpuMethod('constant-buffer upload target is not writable')
                    write = (target + offset, struct.pack('<I', value))
                    staged.pending_writes.append(write)
                    staged.pending_actions.append(('write', *write))
                    staged.registers[(subchannel, 0x238c)] = offset + 4
                elif staged.bindings.get(subchannel) == 0xb197 and 0x19e0 <= method <= 0x19fc and (method - 0x19e0) % 4 == 0:
                    if value not in (0, 1):
                        raise UnsupportedGpuMethod('unsupported color compression value')
                    staged.registers[(subchannel, method)] = value
                elif staged.bindings.get(subchannel) == 0xb197 and method in (0x1604, 0x165c):
                    valid = (method == 0x1604 and value & ~0x1fff == 0 and value & 3 <= 2) or (method == 0x165c and value in (0, 1))
                    if not valid:
                        raise UnsupportedGpuMethod('unsupported point rendering state')
                    staged.registers[(subchannel, method)] = value
                elif staged.bindings.get(subchannel) == 0xb197 and method == 0x15b4:
                    # deko3d identifies this as CSAA enable; only the disabled profile is supported.
                    if value != 0:
                        raise UnsupportedGpuMethod('unsupported CSAA mode')
                    staged.registers[(subchannel, method)] = value
                elif staged.bindings.get(subchannel) == 0xb197 and method in (0x15d0, 0x12d4, 0xd64, 0x308, 0x218, 0x10fc, 0x1290, 0x12d8, 0x12dc, 0x16a8, 0x1794, 0x1950, 0x193c, 0x810, 0x814, 0xfe8, 0xfec, 0x1340, 0x1344, 0x1348, 0x134c, 0x1350, 0x1358, 0x191c, 0x1920, 0x113c):
                    allowed = {
                        0x15d0: (0, 1, 2, 3, 4, 5, 6, 8, 9, 10, 11),
                        0x12d4: (1, 2, 0x1d00, 0x1d01), 0xd64: tuple(range(9)) + (15,),
                        0x308: (1, 3), 0x218: (0, 0x10, 0x20),
                        0x10fc: (0, 0x10, 0x20), 0x1290: (0, 0x10, 0x20),
                        0x12d8: (0, 0x10, 0x20), 0x12dc: (0, 0x10, 0x20),
                        0x16a8: (0x30003,), 0x1794: (0x20002,), 0x1950: (0, 1, 2),
                        # Only the observed geometry guardband profile is supported.
                        0x193c: (2,), 0x810: (0xd5,), 0x814: (0x40,),
                        0xfe8: (0x16,), 0xfec: (0x40,), 0x1340: (0x8006,),
                        0x1344: (0x4302,), 0x1348: (0x4303,), 0x134c: (0x8006,),
                        0x1350: (0x4001,), 0x1358: (0x4303,), 0x191c: (0x900, 0x901),
                        0x1920: (0x404, 0x405, 0x408), 0x113c: (0, 1, 2),
                    }
                    if value not in allowed[method]:
                        raise UnsupportedGpuMethod(f'unsupported 3D profile value 0x{value:x} for 0x{method:x}')
                    staged.registers[(subchannel, method)] = value
                elif staged.bindings.get(subchannel) == 0xb197 and method == 0x121c:
                    if value >> 28 or (value & 15) > 8:
                        raise UnsupportedGpuMethod('unsupported color target selection')
                    staged.registers[(subchannel, method)] = value
                elif staged.bindings.get(subchannel) == 0xb197 and 0x1700 <= method <= 0x177c and (method - 0x1700) % 4 == 0:
                    staged.registers[(subchannel, method)] = value
                elif staged.bindings.get(subchannel) == 0xb197 and method in B197_STATE_MASKS:
                    if value & ~B197_STATE_MASKS[method]:
                        raise UnsupportedGpuMethod(f'unsupported 3D state value 0x{value:x} for 0x{method:x}')
                    staged.registers[(subchannel, method)] = value
                elif staged.bindings.get(subchannel) == 0xb197 and method in (0x1558, 0xdcc, 0xde4, 0xde8, 0xdf8, 0xdfc, 0x19cc, 0x194c, 0x197c, 0x19c0, 0x192c):
                    valid = ((method in (0x1558, 0xde8, 0x19cc, 0x194c, 0x197c, 0x19c0, 0x192c) and value in (0, 1))
                             or (method == 0xdcc and value <= 0xff)
                             or (method == 0xde4 and value <= 0x3f)
                             or (method == 0xdf8 and value <= 0x1ffff)
                             or (method == 0xdfc and value <= 0x3ffff))
                    if not valid:
                        raise UnsupportedGpuMethod(f'unsupported 3D register value 0x{value:x} for 0x{method:x}')
                    staged.registers[(subchannel, method)] = value
                elif staged.bindings.get(subchannel) == 0xb197 and (
                        (0xa00 <= method <= 0xbf4 and (method - 0xa00) % 32 in (0, 4, 8, 12, 16, 20))
                        or (0xc00 <= method <= 0xcf4 and (method - 0xc00) % 16 in (0, 4))
                        or (0xc08 <= method <= 0xcfc and (method - 0xc08) % 16 in (0, 4))
                        or (0xd00 <= method <= 0xd3c and (method - 0xd00) % 8 in (0, 4))
                        or (0xe00 <= method <= 0xef8 and (method - 0xe00) % 16 in (0, 4, 8))):
                    if 0xe00 <= method <= 0xef8 and (method - 0xe00) % 16 == 0 and value not in (0, 1):
                        raise UnsupportedGpuMethod('unsupported scissor enable value')
                    staged.registers[(subchannel, method)] = value
                elif staged.bindings.get(subchannel) == 0xb197 and method in (0x114, 0x118, 0x11c, 0x120):
                    staged.mme.write(method, value)
                else:
                    raise UnsupportedGpuMethod(
                        f'unsupported GPU method 0x{method:x} subchannel={subchannel} '
                        f'class={staged.bindings.get(subchannel)!r} value=0x{value:x} at 0x{address:x}',
                        partial=staged)
        if macro_parameters:
            raise UnsupportedGpuMethod('unconsumed vertex macro parameters')
        if staged.inline_uploads:
            raise UnsupportedGpuMethod('incomplete compute inline upload')
        return staged
