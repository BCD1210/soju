"""Retail NSP names/icons in the launcher, using a synthetic NSP made with made-up keys."""
import json
import os
import pathlib
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
import switch_library  # noqa: E402

CRATE = ROOT / 'tools/switch/fs/Cargo.toml'


@unittest.skipUnless(shutil.which('cargo'), 'Rust is not installed')
class RetailNames(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        subprocess.run(['cargo', 'build', '--quiet', '--offline', '--release', '--bins', '--examples', '--manifest-path', str(CRATE)], check=True)
        cls.target = ROOT / 'tools/switch/fs/target/release'

    def setUp(self):
        self.tmp = pathlib.Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp)
        self.home = self.tmp / 'home'
        self.home.mkdir()
        shutil.copy(self.target / 'soju-switch-fs', self.home / 'soju-switch-fs')
        os.environ['SOJU_SWITCH_HOME'] = str(self.home)
        self.addCleanup(os.environ.pop, 'SOJU_SWITCH_HOME', None)
        self.base = self.tmp / 'base'
        fixture = self.tmp / 'fixture'
        fixture.mkdir()
        subprocess.run([str(self.target / 'examples/make_fixture'), str(fixture)], check=True)
        self.fixture = fixture
        switch_library.import_file(self.base, fixture / 'Kart.nsp')

    def test_without_keys_asks_for_them(self):
        [game] = switch_library.games(self.base)
        self.assertEqual(game['title'], 'Kart.nsp')
        self.assertIn('import-keys', game['issue'])

    def test_with_keys_shows_korean_name_and_icon(self):
        switch_library.import_keys(self.base, self.fixture / 'prod.keys')
        keys = self.base / 'switch/keys'
        self.assertEqual(stat.S_IMODE(keys.stat().st_mode), 0o700)
        self.assertEqual(stat.S_IMODE((keys / 'prod.keys').stat().st_mode), 0o600)
        [game] = switch_library.games(self.base)
        self.assertEqual(game['title'], '카트 레이서')
        self.assertEqual(game['switch']['title_id'], '0100000000010000')
        self.assertEqual(game['switch']['kind'], 'application')
        self.assertEqual(pathlib.Path(game['artwork']).read_bytes(), b'\xff\xd8\xff\xe0KOREAN-ICON')
        self.assertIn('not supported', game['issue'])
        # Cached: a second listing does not need the tool.
        (self.home / 'soju-switch-fs').unlink()
        self.assertEqual(switch_library.games(self.base)[0]['title'], '카트 레이서')

    def test_bad_key_files_are_refused_without_echo(self):
        bad = self.tmp / 'bad.keys'
        bad.write_text('header_key = 0011223344556677zz\n')
        with self.assertRaises(ValueError) as caught:
            switch_library.import_keys(self.base, bad)
        self.assertNotIn('0011', str(caught.exception))
        no_header = self.tmp / 'nh.keys'
        no_header.write_text('titlekek_00 = ' + '00' * 16 + '\n')
        with self.assertRaises(ValueError):
            switch_library.import_keys(self.base, no_header)
        self.assertFalse((self.base / 'switch/keys/prod.keys').exists())

    def test_cli_never_prints_key_values(self):
        out = subprocess.run([sys.executable, str(ROOT / 'scripts/switch_library.py'), 'import-keys', str(self.fixture / 'prod.keys')],
                             capture_output=True, text=True, env=dict(os.environ, SOJU_BASE=str(self.base)), check=True)
        values = [l.split('=')[1].strip() for l in (self.fixture / 'prod.keys').read_text().splitlines()]
        for v in values:
            self.assertNotIn(v, out.stdout + out.stderr)
        rows = json.loads(subprocess.run([sys.executable, str(ROOT / 'scripts/switch_library.py'), 'list', '--json'],
                                         capture_output=True, text=True, env=dict(os.environ, SOJU_BASE=str(self.base)), check=True).stdout)
        self.assertEqual(rows[0]['title'], '카트 레이서')


if __name__ == '__main__':
    unittest.main()
