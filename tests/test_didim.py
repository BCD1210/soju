"""Didim Labs native games: zip install, library entry, launch argv, tamper and manifest checks."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / 'scripts/didim_games.py'
LIBRARY = ROOT / 'scripts/game-library.py'


def make_zip(folder, manifest, tamper=False, sign=True):
    app = folder / 'Test Game.app'
    (app / 'Contents/MacOS').mkdir(parents=True)
    (app / 'Contents/Resources').mkdir()
    exe = app / 'Contents/MacOS/Test Game'
    exe.write_text('#!/bin/sh\nexit 0\n'); exe.chmod(0o755)
    (app / 'Contents/Info.plist').write_text(
        '<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict>'
        '<key>CFBundleExecutable</key><string>Test Game</string>'
        '<key>CFBundleIdentifier</key><string>com.example.testgame</string></dict></plist>')
    if manifest is not None:
        (app / 'Contents/Resources/soju-game.json').write_text(json.dumps(manifest))
    (app / 'Contents/Resources/cover.png').write_bytes(b'\x89PNG\r\n\x1a\n')
    if sign:
        subprocess.run(['/usr/bin/codesign', '--force', '--deep', '-s', '-', str(app)], check=True, capture_output=True)
    if tamper:
        (app / 'Contents/Resources/cover.png').write_bytes(b'changed')
    archive = folder / 'game.zip'
    subprocess.run(['/usr/bin/ditto', '-c', '-k', '--keepParent', str(app), str(archive)], check=True)
    return archive


GOOD = {'schema': 1, 'id': 'test-game', 'title': 'Test Game', 'publisher': 'Didim Labs',
        'version': '1.0', 'cover': 'cover.png'}


@unittest.skipUnless(sys.platform == 'darwin', 'needs ditto and codesign')
class DidimGames(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(); self.dir = Path(self.tmp.name)
        self.base = self.dir / 'base'
        self.env = dict(os.environ, SOJU_BASE=str(self.base))

    def tearDown(self):
        self.tmp.cleanup()

    def run_script(self, *args):
        return subprocess.run([sys.executable, str(SCRIPT), *args], env=self.env, capture_output=True, text=True)

    def test_install_list_launch_remove(self):
        archive = make_zip(self.dir, GOOD)
        subprocess.run(['/usr/bin/xattr', '-w', 'com.apple.quarantine', '0083;00000000;Safari;', str(archive)], check=True)
        result = self.run_script('install', str(archive))
        self.assertEqual(result.returncode, 0, result.stderr)
        app = self.base / 'didim-games/test-game/Test Game.app'
        self.assertTrue(app.is_dir())
        listed = subprocess.run([sys.executable, str(LIBRARY), 'list', '--base', str(self.base)],
                                capture_output=True, text=True, check=True)
        games = [g for g in json.loads(listed.stdout)['games'] if g['platform'] == 'didim']
        self.assertEqual([g['id'] for g in games], ['didim:test-game'])
        self.assertIsNone(games[0]['issue'])
        self.assertTrue(games[0]['artwork'].endswith('cover.png'))
        argv = subprocess.run([sys.executable, str(LIBRARY), 'launch', 'didim:test-game', '--dry-run', '--base', str(self.base)],
                              capture_output=True, text=True, check=True)
        self.assertEqual(json.loads(argv.stdout), ['/usr/bin/open', '-W', '-n', str(app)])
        self.assertEqual(self.run_script('install', str(archive)).returncode, 0)  # reinstall replaces
        self.assertEqual(self.run_script('remove', 'test-game').returncode, 0)
        self.assertFalse((self.base / 'didim-games/test-game').exists())

    def test_tampered_game_is_rejected(self):
        result = self.run_script('install', str(make_zip(self.dir, GOOD, tamper=True)))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('signature', result.stderr)
        self.assertEqual(list((self.base / 'didim-games').glob('test-game')), [])

    def test_bad_manifests_are_rejected(self):
        for manifest in [None, dict(GOOD, schema=2), dict(GOOD, id='../x'), dict(GOOD, cover='../x.png'), dict(GOOD, title='')]:
            with self.subTest(manifest=manifest):
                folder = Path(tempfile.mkdtemp(dir=self.dir))
                self.assertNotEqual(self.run_script('install', str(make_zip(folder, manifest))).returncode, 0)
        self.assertEqual(list((self.base / 'didim-games').glob('[!.]*')), [])


if __name__ == '__main__':
    unittest.main()
