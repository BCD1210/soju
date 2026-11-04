"""GPU virtual-address and allocation-lifetime checks."""
import pathlib
import struct
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / 'tools' / 'switch'))
from gpu_memory import GpuAddressSpace, GpuChannel, initialize_address_space

class GpuAddressTests(unittest.TestCase):
    def test_address_space_initialize_checks_observed_profile(self):
        request = struct.pack('<IiIIQQQ', 1, 0, 0x10000, 0, 0, 0, 0)
        self.assertIsInstance(initialize_address_space(request), GpuAddressSpace)
        for invalid in (request[:-1], struct.pack('<IiIIQQQ', 1, 0, 1, 0, 0, 0, 0),
                        struct.pack('<IiIIQQQ', 1, 0, 0x10000, 1, 0, 0, 0)):
            with self.assertRaises(ValueError):
                initialize_address_space(invalid)

    def test_large_gpu_va_reservation_does_not_allocate_backing(self):
        gpu = initialize_address_space(struct.pack('<IiIIQQQ', 1, 0, 0x10000, 0, 0, 0, 0))
        first = gpu.reserve(0x10000, 0x10000, 0, 0x10000)
        second = gpu.reserve(1, 0x10000, 0, 0x10000)
        self.assertEqual(gpu.reservations[first], 0x100000000)
        self.assertGreaterEqual(second, first + 0x100000000)
        self.assertEqual(gpu.allocations, {})
        for params in ((0, 0x10000, 0, 0x10000), (1, 0x10000, 1, 0x10000),
                       (1, 0x10000, 0, 3), (0xffffffff, 0x10000, 0, 0x10000)):
            with self.assertRaises(ValueError):
                gpu.reserve(*params)

    def test_channel_binding_rejects_wrong_owner_and_duplicate(self):
        gpu = GpuAddressSpace()
        channel = GpuChannel(owner=7)
        with self.assertRaises(ValueError):
            channel.bind(8, gpu)
        channel.bind(7, gpu)
        self.assertIs(channel.address_space, gpu)
        with self.assertRaises(ValueError):
            channel.bind(7, GpuAddressSpace())

    def test_linear_and_tiled_aliases_preserve_kind_and_backing(self):
        gpu = GpuAddressSpace()
        linear = gpu.map_dynamic(1, 0x400000000, 0x10000, 0x10000, 0, 0, 0)
        tiled = gpu.map_dynamic(1, 0x400000000, 0x10000, 0x10000, 0, 0, 0xfe)
        self.assertNotEqual(linear, tiled)
        self.assertEqual(gpu.resolve(linear, 64), gpu.resolve(tiled, 64))
        self.assertEqual((gpu.kind_for_range(linear, 64), gpu.kind_for_range(tiled, 64)), (0, 0xfe))
        gpu.unmap(tiled, 0x10000)
        self.assertEqual(gpu.kind_for_range(linear, 64), 0)
        with self.assertRaises(ValueError):
            gpu.resolve(tiled, 1)

    def test_small_backing_reserves_big_page_without_exposing_padding(self):
        gpu = GpuAddressSpace()
        first = gpu.map_dynamic(1, 0x400000000, 0x1000, 0x10000, 0, 0)
        second = gpu.map_dynamic(2, 0x400001000, 0x1000, 0x10000, 0, 0)
        self.assertEqual(second, first + 0x10000)
        self.assertEqual(gpu.resolve(first, 0x1000), (1, 0x400000000))
        with self.assertRaises(ValueError):
            gpu.resolve(first + 0x1000, 1)
        with self.assertRaises(ValueError):
            gpu.map(first + 0x1000, 0x1000, 2)

    def test_virtual_channel_setup_and_zcull_bounds(self):
        gpu = GpuAddressSpace()
        gpu.reserve(0x10000, 0x10000, 0, 0x10000)
        address = gpu.map_dynamic(2, 0x400000000, 0x20000, 0x10000, 0, 0)
        self.assertGreaterEqual(address, 0x200000000)
        self.assertEqual(gpu.resolve(address + 0x1000, 0x1000), (2, 0x400001000))
        channel = GpuChannel(7)
        channel.bind(7, gpu)
        channel.bind_nvmap(4)
        channel.allocate_gpfifo(2048, 1, 5)
        self.assertEqual(channel.allocate_object(0xb197, 0), 1)
        channel.set_error_notifier(0, 0, 1, 0)
        channel.bind_zcull(address, 2, 0)
        self.assertEqual(channel.zcull_binding, address)
        for action in (lambda: channel.allocate_gpfifo(2048, 1, 5),
                       lambda: channel.bind_zcull(address + 1, 2, 0),
                       lambda: channel.bind_zcull(address + 0x20000, 2, 0),
                       lambda: channel.set_error_notifier(0, 0, 2, 0),
                       lambda: gpu.map_dynamic(2, 0x400000000, 0x20000, 0x10000, 0, 0x30000)):
            with self.assertRaises(ValueError):
                action()

    def test_map_resolve_and_lifetime(self):
        gpu = GpuAddressSpace()
        gpu.register(1, 0x400000000, 0x3000)
        gpu.map(0x8000, 0x2000, 1, 0x1000)
        self.assertEqual(gpu.resolve(0x8800, 0x100), (1, 0x400001800))
        with self.assertRaises(ValueError):
            gpu.release(1)
        with self.assertRaises(ValueError):
            gpu.resolve(0x9fff, 2)
        gpu.unmap(0x8000, 0x2000)
        gpu.release(1)

    def test_rejects_overlaps_out_of_bounds_and_partial_unmap(self):
        gpu = GpuAddressSpace()
        gpu.register(1, 0x400000000, 0x3000)
        gpu.map(0x8000, 0x2000, 1)
        for action in (lambda: gpu.map(0x9000, 0x1000, 1),
                       lambda: gpu.map(0x10000, 0x3000, 1, 0x1000),
                       lambda: gpu.map((1 << 40) - 0x1000, 0x2000, 1),
                       lambda: gpu.unmap(0x8000, 0x1000)):
            with self.assertRaises(ValueError):
                action()
        self.assertEqual(gpu.resolve(0x8000, 1), (1, 0x400000000))

if __name__ == '__main__':
    unittest.main()
