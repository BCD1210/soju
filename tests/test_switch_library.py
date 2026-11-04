"""Synthetic imports and shell-free Switch library launches."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('game_library', ROOT / 'scripts/game-library.py')
library = importlib.util.module_from_spec(spec)
spec.loader.exec_module(library)
from test_switch_core import nro


class SwitchLibraryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.base = self.root / 'base'
        self.env = dict(os.environ, SOJU_BASE=str(self.base))

    def cli(self, *args):
        return subprocess.run(['bash', str(ROOT / 'scripts/soju'), 'switch', *map(str, args)],
                              env=self.env, text=True, capture_output=True)

    def fixture(self, name='Own $(touch nope).nro', data=None):
        path = self.root / name
        path.write_bytes(nro([0xd40000e1]) if data is None else data)
        return path

    def test_import_list_launch_remove(self):
        source = self.fixture()
        uid = 'switch:' + hashlib.sha256(source.read_bytes()).hexdigest()[:16]
        self.assertEqual(self.cli('import', source).stdout.strip(), uid)
        target = self.base / 'switch/games' / source.name
        self.assertEqual(target.read_bytes(), source.read_bytes())
        self.assertEqual(target.stat().st_mode & 0o777, 0o600)
        rows = json.loads(self.cli('list', '--json').stdout)
        self.assertEqual(rows[0]['id'], uid)
        self.assertEqual(rows[0]['title'], source.name)
        self.assertIn(uid, self.cli('list').stdout)
        lib = library.Library(self.base)
        self.assertEqual(lib.scan()['games'], rows)
        self.assertEqual(lib.command(uid), ['/bin/bash', str(ROOT / 'scripts/switch.sh'), 'play', str(target)])
        self.assertEqual(self.cli('remove', uid).returncode, 0)
        self.assertTrue(source.exists())
        self.assertEqual(json.loads(self.cli('list', '--json').stdout), [])
        with self.assertRaises(ValueError): lib.command(uid)

    def test_nacp_and_bounds(self):
        image = nro([0xd40000e1])
        assets = bytearray(0x38 + 0x300)
        assets[:4] = b'ASET'
        struct.pack_into('<QQ', assets, 0x18, 0x38, 0x300)
        assets[0x38:0x38 + 9] = b'Own title'
        source = self.fixture(data=image + assets)
        self.assertEqual(library.switch_library.title(source), 'Own title')
        struct.pack_into('<Q', assets, 0x18, 0xffffffffffffffff)
        source.write_bytes(image + assets)
        self.assertEqual(library.switch_library.title(source), source.name)

    def test_retail_is_listed_but_refused(self):
        for ext in ('nsp', 'xci'):
            source = self.fixture('Synthetic.' + ext, b'synthetic-' + ext.encode())
            result = self.cli('import', source)
            self.assertEqual(result.returncode, 0, result.stderr)
            with self.assertRaisesRegex(ValueError, 'Retail games are not supported'):
                library.Library(self.base).command(result.stdout.strip())
            self.assertNotEqual(self.cli('play', source).returncode, 0)
        rows = json.loads(self.cli('list', '--json').stdout)
        self.assertTrue(all(g['issue'].startswith(library.switch_library.RETAIL_ISSUE) for g in rows))

    def test_refusals_and_no_overwrite(self):
        source = self.fixture()
        self.assertEqual(self.cli('import', source).returncode, 0)
        source.write_bytes(b'replacement')
        self.assertNotEqual(self.cli('import', source).returncode, 0)
        self.assertEqual((self.base / 'switch/games' / source.name).read_bytes(), nro([0xd40000e1]))
        for path in (self.fixture('bad.txt'), self.root / 'missing.nro', self.root):
            self.assertNotEqual(self.cli('import', path).returncode, 0)
        for uid in ('../../anything', 'switch:0000000000000000'):
            self.assertNotEqual(self.cli('remove', uid).returncode, 0)
        link = self.base / 'switch/games/link.nro'
        link.symlink_to(source)
        self.assertEqual(len(json.loads(self.cli('list', '--json').stdout)), 1)
