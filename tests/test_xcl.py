import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest import mock
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('xcl', ROOT / 'scripts/xcl.py')
xcl = importlib.util.module_from_spec(spec)
spec.loader.exec_module(xcl)


class XCLTests(unittest.TestCase):
    def test_manifest_pins_every_download(self):
        m = json.loads((ROOT / 'resources/xcl.json').read_text())
        for key in ('bundle', 'dxc'):
            self.assertTrue(m[key]['url'].startswith('https://'))
            self.assertGreater(m[key]['size'], 0)
            self.assertEqual(len(m[key]['sha256']), 64)
        self.assertIn(m['bundle']['commit'], m['bundle']['source_url'])
        self.assertIn(m['bundle']['commit'][:7], m['bundle']['url'])

    def test_run_turns_mac_paths_into_windows_ones(self):
        with tempfile.TemporaryDirectory() as d:
            game = Path(d) / 'My Game'
            game.mkdir()
            out = xcl.xcl_args(['run', str(game), '--', '/not/a/windows/path', '-x'])
            self.assertEqual(out[0], 'run')
            self.assertTrue(out[1].startswith('Z:\\') and out[1].endswith('\\My Game'))
            self.assertEqual(out[2:], ['--', '/not/a/windows/path', '-x'])   # game arguments untouched
        self.assertEqual(xcl.xcl_args(['--version']), ['--version'])
        with self.assertRaises(xcl.XCLError):
            xcl.xcl_args(['run'])

    def test_bundle_extraction_refuses_entries_outside_it(self):
        with tempfile.TemporaryDirectory() as d:
            bad = Path(d) / 'bad.tar.xz'
            with tarfile.open(bad, 'w:xz') as t:
                info = tarfile.TarInfo('../evil')
                info.size = 1
                t.addfile(info, io.BytesIO(b'x'))
            with self.assertRaises(xcl.XCLError):
                xcl.extract_bundle(bad, Path(d) / 'runtime')
            good = Path(d) / 'good.tar.xz'
            with tarfile.open(good, 'w:xz') as t:
                for name, data in (('soju-xcl/xcl.exe', b'MZ'), ('soju-xcl/LICENSE', b'GPL')):
                    info = tarfile.TarInfo(name)
                    info.size = len(data)
                    t.addfile(info, io.BytesIO(data))
            xcl.extract_bundle(good, Path(d) / 'runtime')
            self.assertEqual((Path(d) / 'runtime/xcl.exe').read_bytes(), b'MZ')

    def test_dxc_dlls_come_out_of_backslashed_zip_names(self):
        with tempfile.TemporaryDirectory() as d:
            z = Path(d) / 'dxc.zip'
            with zipfile.ZipFile(z, 'w') as f:
                f.writestr('bin\\x64\\dxcompiler.dll', b'A')
                f.writestr('bin\\x64\\dxil.dll', b'B')
            xcl.extract_dxc(z, Path(d), ['bin\\x64\\dxcompiler.dll', 'bin\\x64\\dxil.dll'])
            self.assertEqual((Path(d) / 'dxcompiler.dll').read_bytes(), b'A')
            self.assertEqual((Path(d) / 'dxil.dll').read_bytes(), b'B')

    def test_run_needs_setup_first(self):
        with tempfile.TemporaryDirectory() as d:
            with mock.patch.multiple(xcl, XCL=Path(d), RUNTIME=Path(d) / 'runtime', BOTTLE=Path(d) / 'bottle'):
                self.assertEqual(xcl.main(['run', d]), 1)
                self.assertEqual(xcl.main(['nonsense']), 1)


if __name__ == '__main__':
    unittest.main()
