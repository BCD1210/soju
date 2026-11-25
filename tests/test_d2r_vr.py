import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('d2r_vr', ROOT / 'scripts/d2r-vr.py')
vr = importlib.util.module_from_spec(spec)
spec.loader.exec_module(vr)


class ManifestTests(unittest.TestCase):
    def test_every_download_is_pinned_https(self):
        m = json.loads((ROOT / 'resources/d2r-vr.json').read_text())
        for key in ('runtime', 'python', 'mod', 'mod_patch', 'loader', 'reshade'):
            self.assertTrue(m[key]['url'].startswith('https://'), key)
            self.assertRegex(m[key]['sha256'], '^[0-9a-f]{64}$', key)
            self.assertGreater(m[key]['size'], 0, key)
        self.assertEqual(m['runtime']['url'],
                         'https://github.com/BCD1210/soju/releases/download/vr-runtime-v1/soju-vr-runtime.tar.xz')
        patch = m['reshade']['patch']
        self.assertEqual(len(bytes.fromhex(patch['old'])), len(bytes.fromhex(patch['new'])))
        self.assertRegex(patch['patched_sha256'], '^[0-9a-f]{64}$')

    def test_mod_patch_source_pin(self):
        spec = vr.manifest()['mod_patch']
        self.assertEqual(vr.digest(ROOT / spec['patch']), spec['patch_sha256'])
        self.assertEqual(spec['base_commit'], '70ad0e665952f1bd829176ba461a85ebb974c3f0')

    def test_stage_mod_patch_verifies_before_replacing_and_keeps_source(self):
        with tempfile.TemporaryDirectory() as d:
            base = Path(d)
            mod = base / 'mod'
            (mod / 'bin').mkdir(parents=True)
            target = mod / 'bin/d2rl-pcvr.dll'
            target.write_bytes(b'upstream')
            plugin = base / 'patched.dll'
            plugin.write_bytes(b'patched fixture')
            spec = dict(vr.manifest()['mod_patch'], size=plugin.stat().st_size,
                        sha256=vr.digest(plugin))
            vr.stage_mod_patch(mod, plugin, spec)
            self.assertEqual(target.read_bytes(), plugin.read_bytes())
            self.assertEqual(vr.digest(mod / 'soju-source/vr-character-create.patch'), spec['patch_sha256'])
            self.assertIn(spec['base_commit'], (mod / 'soju-source/SOURCE.txt').read_text())
            self.assertTrue((mod / 'soju-source/LICENSE').is_file())
            target.write_bytes(b'keep')
            for bad in (dict(spec, size=1), dict(spec, sha256='0' * 64),
                        dict(spec, patch_sha256='0' * 64)):
                with self.assertRaises(vr.VRError):
                    vr.stage_mod_patch(mod, plugin, bad)
                self.assertEqual(target.read_bytes(), b'keep')

    def test_cli_routes_vr_to_the_script(self):
        self.assertIn('vr)             shift; exec python3 "$ROOT/scripts/d2r-vr.py" "$@" ;;',
                      (ROOT / 'scripts/soju').read_text())

    def test_app_actions_are_routed(self):
        action = (ROOT / 'scripts/desktop-action.sh').read_text()
        self.assertIn('setup|play) exec python3 "$ROOT/scripts/d2r-vr.py" "$1"', action)
        self.assertIn('status) exec python3 "$ROOT/scripts/d2r-vr.py" status --json', action)
        self.assertIn('play --flat --view "${2:-first}"', action)
        library = (ROOT / 'app/Library.swift').read_text()
        sheet = (ROOT / 'app/VRSheet.swift').read_text()
        self.assertIn('model.run(["vr", "setup"]', sheet)
        self.assertIn('task(flat ? ["vr", "flat", view] : ["vr", "play"]', library)
        self.assertIn('model.playVR(game, flat: true, view: flatView)', sheet)
        self.assertIn('D2RVRSheet(model: model, game: game', library)
        self.assertIn('VRSheet.swift', (ROOT / 'scripts/build-app.sh').read_text())
        # the sheet reads these fields from `soju vr status --json`
        for field in ('installed', 'current', 'source_build', 'source_supported'):
            self.assertIn(field, sheet)

    def test_flat_play_sets_the_mods_mode_view_and_mouse_look(self):
        with tempfile.TemporaryDirectory() as d:
            game = Path(d)
            ini = game / 'mods/D2R-PCVR/d2rloader/plugins/d2r_pcvr.ini'
            reshade = game / 'mods/D2R-PCVR/reshade/ReShade.ini'
            ini.parent.mkdir(parents=True)
            reshade.parent.mkdir(parents=True)
            ini.write_bytes(b'[camera]\r\npitch = -30\r\n[stereo]\r\nplatform = 9\r\n[mode]\r\nplatform = 1\r\nflat_view = 3\r\nvr_mode = 4\r\n'
                            b'\r\n[input]\r\nmouse_look = 0\r\nmouse_speed = 0.15\r\n')
            reshade.write_bytes(b'[GENERAL]\r\nPresetPath=.\\D2R_Frame_ReShade.ini\r\n\r\n[INPUT]\r\nKeyOverlay=36,0,0,0\r\n\r\n')
            vr.set_mode(game, True, 'third')
            self.assertEqual(ini.read_bytes(),
                             b'[camera]\r\npitch = -85\r\n[stereo]\r\nplatform = 9\r\n[mode]\r\nplatform = 0\r\nflat_view = 2\r\nvr_mode = 4\r\n'
                             b'\r\n[input]\r\nmouse_look = 1\r\nmouse_speed = 0.15\r\n')
            self.assertEqual(reshade.read_bytes(), b'[GENERAL]\r\nPresetPath=.\\D2R_Frame_ReShade.ini\r\n\r\n'
                             b'[INPUT]\r\nKeyOverlay=36,0,0,0\r\n\r\n[OVERLAY]\r\nTutorialProgress=4\r\n')
            vr.set_mode(game, True, 'third')   # nothing left to change
            self.assertEqual(reshade.read_bytes().count(b'TutorialProgress'), 1)
            vr.set_mode(game, True, 'classic')   # the game's own camera: the mouse clicks as usual
            self.assertIn(b'flat_view = 1\r\n', ini.read_bytes())
            self.assertIn(b'mouse_look = 0\r\n', ini.read_bytes())
            vr.set_mode(game, False)   # VR again: the headset, the flat view kept for next time
            self.assertIn(b'[mode]\r\nplatform = 1\r\nflat_view = 1\r\n', ini.read_bytes())
            self.assertIn(b'mouse_look = 0\r\n', ini.read_bytes())
            self.assertIn(b'[camera]\r\npitch = -30\r\n', ini.read_bytes())   # the headset's tilt back
            ini.write_bytes(b'[mode]\nplatform = 1\n')   # no [input] yet: added
            vr.set_mode(game, True)
            self.assertEqual(ini.read_bytes(), b'[mode]\nplatform = 0\nflat_view = 3\n\n[input]\nmouse_look = 1\n\n[camera]\npitch = -85\n')
            vr.set_mode(Path(d) / 'missing', True)   # no mod installed: nothing to do, no error

    def test_dlss_follows_the_metalfx_bridge(self):
        with tempfile.TemporaryDirectory() as d:
            saves = Path(d)
            settings = saves / 'Settings.json'
            settings.write_text(json.dumps({'Screen Resolution (Windowed)': '3440x1440', 'Window Mode': 2, 'NVIDIA DLSS': 2}))
            vr.fix_settings(saves, flat=True, dlss=True)   # Mac screen, bridge ready: all as the player chose
            self.assertEqual(json.loads(settings.read_text()),
                             {'Screen Resolution (Windowed)': '3440x1440', 'Window Mode': 2, 'NVIDIA DLSS': 2})
            vr.fix_settings(saves, flat=True, dlss=False)  # no Apple shim on this engine: DLSS off
            self.assertEqual(json.loads(settings.read_text())['NVIDIA DLSS'], 0)
            self.assertEqual(json.loads(settings.read_text())['Window Mode'], 2)

    def test_dlss_bridge_puts_the_unwrapper_in_front_of_apples_shim(self):
        from unittest import mock
        with tempfile.TemporaryDirectory() as d:
            base = Path(d)
            engine, vrdir = base / 'engine', base / 'd2r-vr'
            win, unix = engine / 'lib/wine/x86_64-windows', engine / 'lib/wine/x86_64-unix'
            for folder in (win, unix, engine / 'lib/external', vrdir / 'dlss', vrdir / 'bottle/drive_c/windows/system32'):
                folder.mkdir(parents=True)
            (engine / 'lib/external/libd3dshared.dylib').write_bytes(b'x')
            (win / 'nvngx.dll').write_bytes(b'MZ apple D3DMetalDLLsBase shim')
            (vrdir / 'dlss/nvngx.dll').write_bytes(b'MZ soju unwrapper')
            (vrdir / 'bottle/system.reg').write_text('[Software\\\\NVIDIA Corporation\\\\Global\\\\NGXCore] 1\n')
            m = {'ngx_unwrap': {'sha256': vr.digest(vrdir / 'dlss/nvngx.dll')}}
            with mock.patch.multiple(vr, ENGINE=engine, VR=vrdir, BOTTLE=vrdir / 'bottle'), \
                    mock.patch.object(vr, 'wine') as wine:
                self.assertTrue(vr.dlss_bridge(m))
                wine.assert_not_called()   # the copy already has the NGXCore key
            system32 = vrdir / 'bottle/drive_c/windows/system32'
            self.assertEqual((system32 / 'nvngx.dll').read_bytes(), b'MZ soju unwrapper')
            self.assertEqual((system32 / 'nvngx_metalfx.dll').read_bytes(), b'MZ apple D3DMetalDLLsBase shim')
            self.assertEqual((win / 'nvngx_metalfx.dll').read_bytes(), b'MZ apple D3DMetalDLLsBase shim')
            self.assertEqual(os.readlink(unix / 'nvngx_metalfx.so'), '../../external/libd3dshared.dylib')
            (win / 'nvngx.dll').write_bytes(b'MZ wine placeholder')   # no Apple shim: no DLSS
            with mock.patch.multiple(vr, ENGINE=engine, VR=vrdir, BOTTLE=vrdir / 'bottle'):
                self.assertFalse(vr.dlss_bridge(m))
        env = vr.play_env(True, dlss=True)
        self.assertEqual(env['D3DM_ENABLE_METALFX'], '1')
        self.assertEqual(env['WINEDLLOVERRIDES'], 'nvngx=n')
        self.assertNotIn('D3DM_ENABLE_METALFX', vr.play_env(True))

    def test_user_folder_links_become_folders_in_the_copy(self):
        with tempfile.TemporaryDirectory() as d:
            mac_docs = Path(d) / 'mac/Documents'
            mac_docs.mkdir(parents=True)
            (mac_docs / 'keep.txt').write_text('mine')
            bottle = Path(d) / 'bottle'
            user = bottle / 'drive_c/users/crossover'
            (user / 'Saved Games').mkdir(parents=True)
            (user / 'Documents').symlink_to(mac_docs)
            (user / 'Inside').symlink_to(user / 'Saved Games')
            self.assertEqual(vr.isolate_user_folders(bottle), ['Documents'])
            self.assertFalse((user / 'Documents').is_symlink())
            self.assertTrue((user / 'Documents').is_dir())
            self.assertEqual(list((user / 'Documents').iterdir()), [])
            self.assertEqual((mac_docs / 'keep.txt').read_text(), 'mine')
            self.assertTrue((user / 'Inside').is_symlink())
            self.assertEqual(vr.isolate_user_folders(bottle), [])

    def test_flat_play_leaves_the_vr_runtime_out(self):
        with mock.patch.dict(vr.os.environ, {'XR_RUNTIME_JSON': '/x.json'}):
            flat = vr.play_env(True)
            headset = vr.play_env(False)
        for key in ('WINEDLLPATH', 'OXR_BRIDGE_ENV_FILE', 'XR_RUNTIME_JSON'):
            self.assertNotIn(key, flat)
        self.assertEqual(headset['WINEDLLPATH'], str(vr.VR / 'runtime/wine'))
        self.assertEqual(headset['OXR_BRIDGE_ENV_FILE'], str(vr.VR / 'bridge.env'))
        self.assertEqual(flat['WINEPREFIX'], str(vr.BOTTLE))


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

    def test_builds_checked_since_are_accepted(self):
        m = vr.manifest()
        self.assertEqual(vr.builds(m)[0], m['game_build'])
        self.assertIn('3.4.95032', vr.builds(m))
        for extra in m['extra_builds']:
            self.assertTrue(extra['checked'].strip())   # every added build says what was checked

    def test_the_mods_installer_takes_a_listed_build(self):
        with tempfile.TemporaryDirectory() as d:
            mod = Path(d)
            (mod / 'steamvr').mkdir()
            gate = mod / 'steamvr/game_install.py'
            gate.write_bytes(b"BATTLENET_VERSIONS = ('3.3.93847',)\r\nSTEAM = 1\r\n")
            for _ in range(2):
                vr.allow_installer_build(mod, '3.4.95032')
            self.assertEqual(gate.read_bytes(),
                             b"BATTLENET_VERSIONS = ('3.4.95032', '3.3.93847',)\r\nSTEAM = 1\r\n")
            vr.allow_installer_build(mod, '3.3.93847')   # already there: unchanged
            self.assertEqual(gate.read_bytes().count(b'3.3.93847'), 1)

    def test_loader_warning_is_answered_for_the_new_version(self):
        with tempfile.TemporaryDirectory() as d:
            game = Path(d)
            for _ in range(2):
                vr.quiet_loader_warning(game, '3.4.95032')
            state = json.loads((game / 'd2rloader/data/loader-state.json').read_text())
            self.assertEqual(state['ignored_unsupported_versions'], ['3.4.0'])
            (game / 'd2rloader/data/loader-state.json').write_text('{"ignored_unsupported_versions": ["3.3.0"], "x": 1}')
            vr.quiet_loader_warning(game, '3.4.95032')
            state = json.loads((game / 'd2rloader/data/loader-state.json').read_text())
            self.assertEqual(state, {'ignored_unsupported_versions': ['3.3.0', '3.4.0'], 'x': 1})

    def test_windows_paths_go_through_z(self):
        self.assertTrue(vr.winpath('/tmp').startswith('Z:\\'))


if __name__ == '__main__':
    unittest.main()
