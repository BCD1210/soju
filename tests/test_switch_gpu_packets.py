"""Validation of bounded Maxwell GPFIFO packet decoding."""
import pathlib
import struct
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / 'tools' / 'switch'))
from gpu_packets import decode_entry, decode_packets, gpu_characteristics, gpu_metadata_ioctl, inspect_submission

class GpuPacketTests(unittest.TestCase):
    def test_characteristics_inline_response_and_sentinel(self):
        request = bytearray(176)
        struct.pack_into('<QQ', request, 0, 160, 1)
        reply = gpu_characteristics(request)
        self.assertEqual(struct.unpack_from('<QQ', reply), (160, 1))
        self.assertEqual(struct.unpack_from('<I', reply, 0x10)[0], 0x120)
        self.assertEqual(struct.unpack_from('<I', reply, 0x6c)[0], 0xb197)
        with self.assertRaises(ValueError):
            gpu_characteristics(bytes(176))
        with self.assertRaises(ValueError):
            gpu_characteristics(bytes(175))

    def test_gpu_metadata_read_only_buffer_validation(self):
        self.assertEqual(struct.unpack('<I', gpu_metadata_ioctl(0x80044701, b'', 4)), (0x1000,))
        self.assertEqual(len(gpu_metadata_ioctl(0x80284702, b'', 40)), 40)
        for ioctl, data, size in ((0x80044701, b'x', 4), (0x80044701, b'', 3),
                                  (0x80284702, b'', 4), (0xffffffff, b'', 4)):
            with self.assertRaises(ValueError):
                gpu_metadata_ioctl(ioctl, data, size)

    def test_submission_snapshot_validation(self):
        args = struct.pack('<QIIII', 0x8000, 1, 0x104, 5, 2)
        entries = struct.pack('<II', 0x4000, 2 << 10)
        words = struct.pack('<II', 0x60010000, 0xb197)
        fields, decoded = inspect_submission(args, entries, 8, lambda addr, size: words)
        self.assertEqual(fields, (0x8000, 0x104, 5, 2))
        self.assertEqual(decoded[0][3], [(0, 0, 0xb197)])
        with self.assertRaises(ValueError):
            inspect_submission(args, entries[:-1], 8, lambda addr, size: words)
        with self.assertRaises(ValueError):
            inspect_submission(args, entries, 8, lambda addr, size: b'')
        invalid = struct.pack('<QIIII', 0x8000, 1, 0x200, 5, 2)
        with self.assertRaises(ValueError):
            inspect_submission(invalid, entries, 8, lambda addr, size: words)

    def test_method_modes(self):
        for header, methods in ((0x20034100, [0x100, 0x101, 0x102]),
                                (0x60034100, [0x100, 0x100, 0x100]),
                                (0xa0034100, [0x100, 0x101, 0x101])):
            self.assertEqual(list(decode_packets([header, 10, 20, 30])),
                             [(2, method * 4, value) for method, value in zip(methods, (10, 20, 30))])
        self.assertEqual(list(decode_packets([0x81234100])), [(2, 0x400, 0x123)])
        self.assertEqual(list(decode_packets([0, 0x60010000, 0xb197])), [(0, 0, 0xb197)])

    def test_rejects_truncation_modes_and_overflow(self):
        for words in ([0x20034100, 1], [0xc0000000], [0xe0000000], [0x20021000, 1, 2],
                      [0x20020fff, 1, 2]):
            with self.assertRaises(ValueError):
                list(decode_packets(words))

    def test_entry_flags_and_boundaries(self):
        self.assertIsNone(decode_entry(0, 0))
        self.assertEqual(decode_entry(0x12345000, 0x80020e01), (0x112345000, 131, True, True))
        for low, high in ((1, 0x400), (0, 0x500), (0, 1), (0xfffffffc, 0xff | (2 << 10))):
            with self.assertRaises(ValueError):
                decode_entry(low, high)

if __name__ == '__main__':
    unittest.main()
