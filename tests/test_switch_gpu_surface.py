"""Block-linear clear changes visible texels but preserves padded GOBs."""
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / 'tools' / 'switch'))
from gpu_surface import plan_uniform_clear, plan_pitch_to_tiled


class GpuSurfaceTests(unittest.TestCase):
    def test_clear_matches_texel_swizzle_and_keeps_padding(self):
        width, height, block_gobs = 32, 136, 16
        pitch = width * 4
        data = bytearray(b'\x77' * (pitch * 256))
        pixel = bytes((0, 0, 0, 255))
        for offset, chunk in plan_uniform_clear(0, width, height, 4, pixel):
            data[offset:offset+len(chunk)] = chunk
        touched = set()
        for y in range(height):
            for x in range(0, pitch, 4):
                offset = ((y // 128) * pitch * 128 + (x // 64) * 512 * block_gobs
                          + ((y // 8) % block_gobs) * 512 + ((x % 64) // 32) * 256
                          + ((y % 8) // 2) * 64 + ((x % 32) // 16) * 32
                          + (y % 2) * 16 + x % 16)
                self.assertEqual(data[offset:offset+4], pixel)
                touched.update(range(offset, offset + 4))
        self.assertTrue(all(value == 0x77 for i, value in enumerate(data) if i not in touched))

    def test_pitch_to_tiled_copies_pixels_and_preserves_padding(self):
        width, height, pitch, block_gobs = 17, 9, 128, 2
        source = bytes((i % 251 for i in range(width * height * 4)))
        tiled = bytearray(b'\x77' * (pitch * 16))
        target_base = 0x10000
        for src, dst, size in plan_pitch_to_tiled(0, target_base, width, height, width * 4, 1):
            tiled[dst-target_base:dst-target_base+size] = source[src:src+size]
        touched = set()
        for y in range(height):
            for x in range(width * 4):
                offset = ((y // 16) * pitch * 16 + (x // 64) * 512 * block_gobs
                          + ((y // 8) % block_gobs) * 512 + ((x % 64) // 32) * 256
                          + ((y % 8) // 2) * 64 + ((x % 32) // 16) * 32
                          + (y % 2) * 16 + x % 16)
                self.assertEqual(tiled[offset], source[y * width * 4 + x])
                touched.add(offset)
        self.assertTrue(all(value == 0x77 for i, value in enumerate(tiled) if i not in touched))

    def test_rejects_partial_and_overflow_geometry(self):
        for args in ((0, 15, 8, 4, bytes(4)), (0, 16, 7, 4, bytes(4)),
                     ((1 << 40) - 4, 16, 8, 4, bytes(4)), (0, 16, 8, 6, bytes(4))):
            with self.assertRaises(ValueError):
                plan_uniform_clear(*args)


if __name__ == '__main__':
    unittest.main()
