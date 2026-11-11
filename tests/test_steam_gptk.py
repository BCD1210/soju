import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('library', ROOT / 'scripts/game-library.py')
library = importlib.util.module_from_spec(spec)
spec.loader.exec_module(library)
CLIENT = 'drive_c/Program Files (x86)/Steam/steam.exe'


class SteamGPTKTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="soju steam gptk ' ")
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)

    def put(self, relative):
        path = self.base / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text('')

    def gptk(self, *args):
        env = dict(os.environ, SOJU_BASE=str(self.base))
        return subprocess.run(['bash', str(ROOT / 'scripts/steam-gptk.sh'), *args], env=env,
                              capture_output=True, text=True, timeout=60)

    def test_status_is_off_until_flag_and_bottle(self):
        self.assertEqual(self.gptk('status').stdout.strip(), 'off')
        self.put('steam-gptk')
        self.assertEqual(self.gptk('status').stdout.strip(), 'off')   # no bottle yet
        self.put('steam-gptk-bottle/' + CLIENT)
        self.assertEqual(self.gptk('status').stdout.strip(), 'on')

    def test_on_needs_steam_installed(self):
        result = self.gptk('on')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('install Steam first', result.stderr)
        self.assertFalse((self.base / 'steam-gptk').exists())

    def test_library_reads_the_bottle_steam_runs_from(self):
        self.put('steam-bottle/' + CLIENT)
        self.assertEqual(library.Library(self.base).folders['steam'], 'steam-bottle')
        self.put('steam-gptk-bottle/' + CLIENT)
        self.assertEqual(library.Library(self.base).folders['steam'], 'steam-bottle')   # not switched on
        self.put('steam-gptk')
        self.assertEqual(library.Library(self.base).folders['steam'], 'steam-gptk-bottle')

    def test_play_sh_picks_the_gptk_bottle(self):
        text = (ROOT / 'scripts/play.sh').read_text()
        self.assertIn('steam-gptk.sh" status', text)
        self.assertIn('steam-gptk-engine.sh', text)
        # The GPTK branch must keep the webhelper wrapper: without it Steam's window is black.
        branch = text.split('SOJU_STEAM_GPTK:-0}" = 1 ]; then', 1)[1].split('WINESTABLE=', 1)[0]
        self.assertIn('steamwebhelper-wrapper.exe', branch)
        self.assertIn('-no-cef-sandbox', branch)

    def test_engine_keeps_apple_shims(self):
        text = (ROOT / 'scripts/steam-gptk-engine.sh').read_text()
        self.assertIn('winemac.so', text)
        for swapped in ('d3d11.dll', 'dxgi.dll', 'dxmt'):
            self.assertNotIn(swapped, text.split('cp -Rc', 1)[1])


if __name__ == '__main__':
    unittest.main()
