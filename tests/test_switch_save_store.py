"""Atomic save persistence and hostile/corrupt snapshot checks."""
import base64
import json
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / 'tools' / 'switch'))
from memory_files import MemoryFiles
from save_store import SaveStore

class SaveStoreTests(unittest.TestCase):
    def test_round_trip_and_isolation(self):
        with tempfile.TemporaryDirectory() as tmp:
            store = SaveStore(tmp, 'a' * 64)
            files = MemoryFiles()
            files.create('/scores/best', 4)
            files.write('/scores/best', 0, b'1234')
            attr = bytearray(64)
            attr[8], attr[0x20] = 1, 1
            attr = bytes(attr)
            record = {'dirs': {'/', '/scores'}, 'files': files, 'id': 1, 'space': 1, 'size': 100}
            self.assertEqual(store.load(), {})
            store.save({attr: record})
            loaded = store.load()
            self.assertEqual(loaded[attr]['files'].read('/scores/best', 0, 4), b'1234')
            self.assertEqual(loaded[attr]['dirs'], {'/', '/scores'})
            with self.assertRaises(ValueError):
                store.save({attr: {**record, 'size': 3}})
            self.assertEqual(store.load()[attr]['files'].read('/scores/best', 0, 4), b'1234')
            self.assertEqual(SaveStore(tmp, 'b' * 64).load(), {})

    def test_corrupt_traversal_and_symlink_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            store = SaveStore(tmp, 'a' * 64)
            store.directory.mkdir(parents=True)
            attr = bytearray(64)
            attr[8], attr[0x20] = 1, 1
            invalid = {'version': 1, 'saves': [{'attribute': bytes(attr).hex(), 'dirs': ['/'],
                       'files': {'/../outside': base64.b64encode(b'x').decode()},
                       'id': 1, 'space': 1, 'size': 1}]}
            store.path.write_text(json.dumps(invalid))
            with self.assertRaises(ValueError):
                store.load()
            store.path.unlink()
            self.assertEqual(store.load(), {})
            store.path.symlink_to(pathlib.Path(tmp) / 'outside')
            with self.assertRaises(ValueError):
                store.save({})
            self.assertFalse((pathlib.Path(tmp) / 'outside').exists())

    def test_concurrent_instances_are_exclusively_locked(self):
        with tempfile.TemporaryDirectory() as tmp:
            first, second = SaveStore(tmp, 'c' * 64), SaveStore(tmp, 'c' * 64)
            self.assertEqual(first.load(), {})
            with self.assertRaises(BlockingIOError):
                second.load()
            first.close()
            self.assertEqual(second.load(), {})
            second.close()

if __name__ == '__main__':
    unittest.main()
