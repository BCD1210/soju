"""Tests for virtual directory enumeration."""
import pathlib
import struct
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / 'tools' / 'switch'))
from virtual_directory import Directory, ENTRY_SIZE

class DirectoryTests(unittest.TestCase):
    def test_entries_are_paginated_and_described(self):
        directory = Directory('/switch', 3, {'/', '/switch', '/switch/games'}, 123)
        first, count = directory.read(ENTRY_SIZE)
        self.assertEqual(count, 1)
        self.assertEqual(first.split(b'\0', 1)[0], b'games')
        self.assertEqual(first[0x304], 0)
        second, count = directory.read(ENTRY_SIZE)
        self.assertEqual(count, 1)
        self.assertEqual(second.split(b'\0', 1)[0], b'soju-current.nro')
        self.assertEqual(second[0x304], 1)
        self.assertEqual(struct.unpack_from('<q', second, 0x308)[0], 123)
        self.assertEqual(directory.read(ENTRY_SIZE), (b'', 0))

    def test_filter_and_invalid_mode(self):
        dirs = {'/', '/switch', '/switch/games'}
        self.assertEqual(len(Directory('/switch', 1, dirs, 100).entries), 1)
        self.assertEqual(len(Directory('/switch', 2, dirs, 100).entries), 1)
        with self.assertRaises(ValueError):
            Directory('/switch', 4, dirs, 100)

if __name__ == '__main__':
    unittest.main()
