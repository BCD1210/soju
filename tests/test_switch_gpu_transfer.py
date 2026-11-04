"""Bounded GPU transfer execution checks."""
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / 'tools' / 'switch'))
from gpu_transfer import plan_linear_copy, commit_linear_copy


class GpuTransferTests(unittest.TestCase):
    def test_pitched_copy_preserves_padding(self):
        mem = bytearray(b'abcd....efgh....' + b'?' * 16)
        def read(addr, size):
            return mem[addr:addr + size]
        def valid(addr, size):
            return 16 <= addr and addr + size <= len(mem)
        plan = plan_linear_copy(0, 16, 4, 2, 8, 8, read, valid)
        self.assertEqual(mem[16:], b'?' * 16)
        def write(addr, data):
            mem[addr:addr + len(data)] = data
        commit_linear_copy(plan, write)
        self.assertEqual(mem[16:], b'abcd????efgh????')

    def test_invalid_copy_does_not_write(self):
        mem = bytearray(b'a' * 32)
        for geometry, validator in (((0, 4, 8, 1, 8, 8), lambda a, n: True),
                                    ((0, 16, 4, 2, 8, 8), lambda a, n: a == 16),
                                    ((0, 16, 9, 1, 8, 9), lambda a, n: True)):
            with self.assertRaises(ValueError):
                plan_linear_copy(*geometry, lambda a, n: mem[a:a+n], validator)
        self.assertEqual(mem, bytearray(b'a' * 32))


if __name__ == '__main__':
    unittest.main()
