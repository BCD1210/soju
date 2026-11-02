import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('d2r_vr', ROOT / 'scripts/d2r-vr.py')
vr = importlib.util.module_from_spec(spec)
spec.loader.exec_module(vr)


class ManifestTests(unittest.TestCase):
    def test_every_download_is_pinned_https(self):
        m = json.loads((ROOT / 'resources/d2r-vr.json').read_text())
        for key in ('runtime', 'python', 'mod', 'loader', 'reshade'):
            self.assertTrue(m[key]['url'].startswith('https://'), key)
            self.assertRegex(m[key]['sha256'], '^[0-9a-f]{64}$', key)
            self.assertGreater(m[key]['size'], 0, key)
        self.assertEqual(m['runtime']['url'],
                         'https://github.com/BCD1210/soju/releases/download/vr-runtime-v1/soju-vr-runtime.tar.xz')
        patch = m['reshade']['patch']
        self.assertEqual(len(bytes.fromhex(patch['old'])), len(bytes.fromhex(patch['new'])))
        self.assertRegex(patch['patched_sha256'], '^[0-9a-f]{64}$')

    def test_cli_routes_vr_to_the_script(self):
        self.assertIn('vr)             shift; exec python3 "$ROOT/scripts/d2r-vr.py" "$@" ;;',
                      (ROOT / 'scripts/soju').read_text())

    def test_app_actions_are_routed(self):
        action = (ROOT / 'scripts/desktop-action.sh').read_text()
        self.assertIn('setup|play|status) exec python3 "$ROOT/scripts/d2r-vr.py" "$1"', action)
        library = (ROOT / 'app/Library.swift').read_text()
        self.assertIn('model.run(["vr", "setup"]', library)
        self.assertIn('task(["vr", "play"]', library)


class ReShadePatchTests(unittest.TestCase):
    def spec_for(self, data, at):
        old, new = b'\x48\x8b\xcf\x48\x8b\x07\xff\x50\x50', b'\xe9\x18\x04\x00\x00\x90\x90\x90\x90'
        patched = bytearray(data)
        patched[at:at + len(new)] = new
        return {'dll_sha256': hashlib.sha256(data).hexdigest(),
                'patch': {'file_offset': at, 'old': old.hex(), 'new': new.hex(),
                          'patched_sha256': hashlib.sha256(bytes(patched)).hexdigest()}}

    def test_patches_the_pinned_build_once(self):
        data = b'\0' * 100 + bytes.fromhex('488bcf488b07ff5050') + b'\1' * 50
        spec = self.spec_for(data, 100)
        with tempfile.TemporaryDirectory() as d:
            dll = Path(d) / 'ReShade64.dll'
            dll.write_bytes(data)
            self.assertTrue(vr.patch_reshade(dll, spec))
            self.assertEqual(hashlib.sha256(dll.read_bytes()).hexdigest(), spec['patch']['patched_sha256'])
            self.assertFalse(vr.patch_reshade(dll, spec))   # already patched: left alone

    def test_refuses_another_build(self):
        data = b'\0' * 100 + bytes.fromhex('488bcf488b07ff5050')
        spec = self.spec_for(data, 100)
        with tempfile.TemporaryDirectory() as d:
            dll = Path(d) / 'ReShade64.dll'
            dll.write_bytes(data + b'x')
            with self.assertRaises(vr.VRError):
                vr.patch_reshade(dll, spec)
            self.assertEqual(dll.read_bytes(), data + b'x')


class HelperTests(unittest.TestCase):
    def test_zip_extraction_refuses_escaping_paths(self):
        with tempfile.TemporaryDirectory() as d:
            archive = Path(d) / 'a.zip'
            with zipfile.ZipFile(archive, 'w') as z:
                z.writestr('../evil.txt', 'x')
            with self.assertRaises(vr.VRError):
                vr.safe_extract_zip(archive, Path(d) / 'out')
            self.assertFalse((Path(d) / 'evil.txt').exists())

    def test_game_build_reads_build_info(self):
        with tempfile.TemporaryDirectory() as d:
            game = Path(d) / vr.GAME_REL
            game.mkdir(parents=True)
            (game / '.build.info').write_text('Branch!STRING:0|Active!DEC:1|Version!STRING:0\n'
                                              'kr|1|3.3.93847\n')
            self.assertEqual(vr.game_build(Path(d)), '3.3.93847')
            self.assertIsNone(vr.game_build(Path(d) / 'missing'))

    def test_settings_fixed_at_the_first_swapchain_size(self):
        with tempfile.TemporaryDirectory() as d:
            settings = Path(d) / 'Settings.json'
            settings.write_text('\ufeff' + json.dumps({'Screen Resolution (Windowed)': '1731x932', 'Window Mode': 1}),
                                encoding='utf-8')
            vr.fix_settings(Path(d))
            data = json.loads(settings.read_text(encoding='utf-8-sig'))
            self.assertEqual(data['Screen Resolution (Windowed)'], '1920x1080')
            self.assertEqual(data['Window Mode'], 0)

    def test_running_ignores_shells_that_only_mention_the_game(self):
        from unittest import mock
        ps = ('/bin/zsh -c pgrep -fl "D2RLoader.exe"\n'
              'bash /opt/soju/scripts/play.sh d2r D2R.exe\n'
              'C:\\Program Files (x86)\\Diablo II Resurrected\\D2R.exe -uid osi\n'
              'D2RLoader.exe -mod D2R-PCVR\n')
        with mock.patch.object(vr.subprocess, 'run', return_value=mock.Mock(stdout=ps)):
            found = vr.running(['D2R.exe', 'D2RLoader.exe'])
        self.assertEqual(len(found), 2)
        self.assertTrue(all('zsh' not in line and 'bash' not in line for line in found))

    def test_refresh_keeps_the_copy_when_the_new_build_is_unsupported(self):
        from unittest import mock
        with tempfile.TemporaryDirectory() as d:
            base = Path(d)
            source, vr_dir = base / 'bottle', base / 'd2r-vr'
            bottle = vr_dir / 'bottle'
            for b, build in ((source, '3.3.99999'), (bottle, '3.3.93847')):
                game = b / vr.GAME_REL
                game.mkdir(parents=True)
                (game / 'D2R.exe').write_bytes(b'')
                (game / '.build.info').write_text('Version!STRING:0\n' + build + '\n')
            engine = base / 'engine'
            (engine / 'bin').mkdir(parents=True)
            (engine / 'bin/wine').write_bytes(b'')
            with mock.patch.multiple(vr, ENGINE=engine, SOURCE=source, VR=vr_dir, BOTTLE=bottle,
                                     STATE=vr_dir / 'state.json'), \
                    mock.patch.object(vr, 'running', return_value=[]):
                with self.assertRaises(vr.VRError) as caught:
                    vr.setup(refresh=True)
            self.assertIn('was kept', str(caught.exception))
            self.assertTrue((bottle / vr.GAME_REL / 'D2R.exe').exists())

    def test_windows_paths_go_through_z(self):
        self.assertTrue(vr.winpath('/tmp').startswith('Z:\\'))


if __name__ == '__main__':
    unittest.main()
