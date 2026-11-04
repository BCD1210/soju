"""Bounded GPU virtual-address mappings; no rendering or host pointer access."""
import struct


def initialize_address_space(payload):
    """Validate the supported nvhost-as-gpu InitializeEx virtual profile."""
    if len(payload) != 40:
        raise ValueError('invalid GPU address-space initialization size')
    flags, as_fd, big_page, reserved, first, second, third = struct.unpack('<IiIIQQQ', payload)
    if (flags != 1 or as_fd != 0 or big_page != 0x10000 or reserved
            or first or second or third):
        raise ValueError('unsupported GPU address-space initialization')
    return GpuAddressSpace()


PAGE = 0x1000
ADDRESS_LIMIT = 1 << 40
MAX_ALLOCATION = 1 << 30

class GpuChannel:
    """Control state only; GPU command submission is not implemented."""
    def __init__(self, owner):
        self.owner = owner
        self.address_space = None
        self.nvmap_fd = None
        self.gpfifo_capacity = None
        self.syncpoint = None
        self.submitted = 0
        self.completed = 0
        self.faulted = False
        self.object_contexts = {}
        self.error_events = []
        self.notifier_enabled = False
        self.priority = 100
        self.zcull_binding = None

    def bind_zcull(self, address, mode, padding):
        if self.address_space is None or self.gpfifo_capacity is None or mode != 2 or padding or address % PAGE:
            raise ValueError('unsupported GPU ZCull binding')
        self.address_space.resolve(address, 0x1000)
        self.zcull_binding = address

    def set_priority(self, priority):
        if priority not in (50, 100, 150):
            raise ValueError('unsupported GPU channel priority')
        self.priority = priority

    def set_error_notifier(self, offset, size, enabled, padding):
        if offset != 0 or size != 0 or enabled not in (0, 1) or padding:
            raise ValueError('unsupported GPU error notifier configuration')
        self.notifier_enabled = bool(enabled)

    def allocate_object(self, class_id, flags):
        if self.gpfifo_capacity is None or class_id != 0xb197 or flags != 0 or len(self.object_contexts) >= 16:
            raise ValueError('unsupported GPU object context')
        object_id = len(self.object_contexts) + 1
        self.object_contexts[object_id] = class_id
        return object_id

    def allocate_gpfifo(self, entries, flags, syncpoint):
        if (self.address_space is None or self.nvmap_fd is None or self.gpfifo_capacity is not None
                or not 1 <= entries <= 65536 or entries & (entries - 1)
                or flags != 1 or syncpoint <= 0):
            raise ValueError('invalid GPU channel GPFIFO allocation')
        self.gpfifo_capacity = entries
        self.syncpoint = syncpoint

    def bind_nvmap(self, fd):
        if fd <= 0 or self.nvmap_fd is not None:
            raise ValueError('invalid or duplicate GPU channel NVMap binding')
        self.nvmap_fd = fd

    def bind(self, owner, address_space):
        if owner != self.owner or self.address_space is not None:
            raise ValueError('invalid or duplicate GPU channel binding')
        if not isinstance(address_space, GpuAddressSpace):
            raise ValueError('GPU address space is not initialized')
        self.address_space = address_space


class GpuAddressSpace:
    def __init__(self):
        self.allocations = {}
        self.mappings = {}
        self.mapping_kinds = {}
        self.footprints = {}  # VA span may exceed actual accessible backing.
        self.reservations = {}

    def reserve(self, pages, page_size, flags, alignment):
        if (pages <= 0 or page_size not in (PAGE, 0x10000) or flags != 0
                or alignment < page_size or alignment & (alignment - 1)):
            raise ValueError('unsupported GPU reservation parameters')
        if len(self.reservations) >= 4096:
            raise ValueError('GPU reservation count exceeds limit')
        size = pages * page_size
        if size > ADDRESS_LIMIT:
            raise ValueError('GPU reservation too large')
        base = (0x100000000 + alignment - 1) & -alignment
        occupied = list(self.reservations.items()) + [(address, self.footprints.get(address, mapping[0])) for address, mapping in self.mappings.items()]
        for address, length in sorted(occupied):
            if base + size <= address:
                break
            if base < address + length:
                base = (address + length + alignment - 1) & -alignment
        if base + size > ADDRESS_LIMIT:
            raise ValueError('GPU virtual address space exhausted')
        self.reservations[base] = size
        return base

    def map_dynamic(self, handle, guest_address, backing_size, page_size, buffer_offset, mapping_size, kind=0):
        if (kind not in (0, 0x11, 0xfe) or page_size not in (PAGE, 0x10000) or guest_address % PAGE or backing_size <= 0
                or backing_size % PAGE or buffer_offset < 0 or buffer_offset % PAGE
                or buffer_offset >= backing_size or len(self.mappings) >= 4096):
            raise ValueError('invalid GPU dynamic mapping parameters')
        size = mapping_size or (backing_size - buffer_offset)
        if size <= 0 or size % PAGE or size > backing_size - buffer_offset:
            raise ValueError(f'GPU dynamic mapping exceeds backing allocation size=0x{size:x} backing=0x{backing_size:x} offset=0x{buffer_offset:x} page=0x{page_size:x}')
        if handle in self.allocations and self.allocations[handle] != (guest_address, backing_size):
            raise ValueError('GPU allocation backing changed')
        address = (0x100000000 + page_size - 1) & -page_size
        span = (size + page_size - 1) & -page_size
        occupied = list(self.reservations.items()) + [(a, self.footprints.get(a, m[0])) for a, m in self.mappings.items()]
        for base, length in sorted(occupied):
            if address + span <= base:
                break
            if address < base + length:
                address = (base + length + page_size - 1) & -page_size
        if address + span > ADDRESS_LIMIT:
            raise ValueError('GPU virtual address space exhausted')
        if handle not in self.allocations:
            self.register(handle, guest_address, backing_size)
        self.map(address, size, handle, buffer_offset)
        self.footprints[address] = span
        self.mapping_kinds[address] = kind
        return address

    def register(self, handle, guest_address, size):
        if handle in self.allocations or handle <= 0:
            raise ValueError('invalid or duplicate GPU allocation')
        if guest_address % PAGE or size <= 0 or size % PAGE or size > MAX_ALLOCATION:
            raise ValueError('invalid GPU allocation bounds')
        if guest_address < 0 or guest_address + size > 1 << 64:
            raise ValueError('guest allocation range overflow')
        self.allocations[handle] = (guest_address, size)

    def map(self, address, size, handle, offset=0):
        if address % PAGE or size <= 0 or size % PAGE or offset % PAGE or offset < 0:
            raise ValueError('unaligned GPU mapping')
        if address < 0 or address + size > ADDRESS_LIMIT:
            raise ValueError('GPU mapping outside address space')
        if handle not in self.allocations:
            raise ValueError('unknown GPU allocation')
        _, allocation_size = self.allocations[handle]
        if offset > allocation_size or size > allocation_size - offset:
            raise ValueError('GPU mapping outside allocation')
        if any(address < base + self.footprints.get(base, length) and base < address + size
               for base, (length, _, _) in self.mappings.items()):
            raise ValueError('overlapping GPU mapping')
        self.mappings[address] = (size, handle, offset)

    def resolve(self, address, size):
        if address < 0 or size <= 0 or address + size > ADDRESS_LIMIT:
            raise ValueError('invalid GPU address range')
        for base, (length, handle, offset) in self.mappings.items():
            if base <= address and address + size <= base + length:
                guest, _ = self.allocations[handle]
                return handle, guest + offset + address - base
        raise ValueError('GPU range is not mapped')

    def kind_for_range(self, address, size):
        self.resolve(address, size)
        for base, (length, _, _) in self.mappings.items():
            if base <= address and address + size <= base + length:
                return self.mapping_kinds.get(base, 0)
        raise ValueError('GPU range is not mapped')

    def unmap(self, address, size):
        mapping = self.mappings.get(address)
        if mapping is None or mapping[0] != size:
            raise ValueError('unmap must match an entire mapping')
        del self.mappings[address]
        self.footprints.pop(address, None)
        self.mapping_kinds.pop(address, None)

    def release(self, handle):
        if handle not in self.allocations:
            raise ValueError('unknown GPU allocation')
        if any(mapped_handle == handle for _, mapped_handle, _ in self.mappings.values()):
            raise ValueError('GPU allocation remains mapped')
        del self.allocations[handle]
