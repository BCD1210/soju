"""Bounds and aliases for virtual shared-font fallback."""
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / 'tools' / 'switch'))
from shared_fonts import build_font_image, MAX_FONT_SIZE


class SharedFontsTests(unittest.TestCase):
    def test_unicode_alias_has_consistent_offsets_and_original_bytes(self):
        standard, unicode_font = b'standard', b'unicode'
        image, records = build_font_image(standard, unicode_font)
        self.assertEqual(records[0], (0, len(standard)))
        self.assertEqual(records[1:], ((0x1000, len(unicode_font)),) * 5)
        self.assertEqual(image[0x1000:0x1000 + len(unicode_font)], unicode_font)

    def test_latin_only_alias_and_invalid_sizes(self):
        image, records = build_font_image(b'a')
        self.assertEqual(image, b'a')
        self.assertEqual(records, ((0, 1),) * 6)
        with self.assertRaises(ValueError):
            build_font_image(b'')
        with self.assertRaises(ValueError):
            build_font_image(b'a', b'x' * (MAX_FONT_SIZE + 1))


if __name__ == '__main__':
    unittest.main()
