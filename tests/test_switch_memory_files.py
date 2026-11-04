"""Tests for bounded volatile guest files."""
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / 'tools' / 'switch'))
from memory_files import MemoryFiles, MAX_FILE_SIZE

class MemoryFilesTests(unittest.TestCase):
    def test_writes_resize_and_eof(self):
        fs = MemoryFiles()
        fs.create('/switch/test', 3)
        fs.write('/switch/test', 1, b'AB')
        self.assertEqual(fs.read('/switch/test', 0, 9), b'\0AB')
        self.assertEqual(fs.read('/switch/test', 99, 1), b'')
        with self.assertRaises(ValueError):
            fs.write('/switch/test', 3, b'Z')
        fs.write('/switch/test', 5, b'Z', append=True)
        self.assertEqual(fs.read('/switch/test', 0, 10), b'\0AB\0\0Z')
        fs.resize('/switch/test', 2)
        self.assertEqual(fs.read('/switch/test', 0, 10), b'\0A')

    def test_rejects_out_of_bounds_and_duplicate(self):
        fs = MemoryFiles()
        fs.create('/x', 0)
        for action in (lambda: fs.create('/x', 0), lambda: fs.create('/y', MAX_FILE_SIZE + 1),
                       lambda: fs.read('/x', -1, 1), lambda: fs.write('/x', -1, b'x')):
            with self.assertRaises(ValueError):
                action()
        self.assertNotIn('/y', fs.files)

if __name__ == '__main__':
    unittest.main()
