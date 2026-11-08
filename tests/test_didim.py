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


class DidimStore(unittest.TestCase):
    """activate() talks to the store; these cover the paths that never touch the network or keychain."""
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(); self.dir = Path(self.tmp.name)
        self.base = self.dir / 'base'
        self.env = dict(os.environ, SOJU_BASE=str(self.base))
        self.addCleanup(self.tmp.cleanup)

    def run_activate(self, key, store='http://127.0.0.1:9'):
        env = dict(os.environ, SOJU_BASE=tempfile.mkdtemp(), SOJU_DIDIM_STORE=store, SOJU_DIDIM_KEY=key)
        return subprocess.run([sys.executable, str(SCRIPT), 'activate', 'no-such-game-for-tests'], env=env,
                              capture_output=True, text=True)

    def test_malformed_key_is_rejected_before_any_request(self):
        result = self.run_activate('not a key!')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('does not look right', result.stderr)

    def test_unreachable_store_is_reported(self):
        result = self.run_activate('DSTC-TEST-0000-0000')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Could not reach the store', result.stderr)

    def serve_catalog(self, catalog):
        import http.server, threading
        body = json.dumps(catalog).encode()
        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                self.send_response(200 if self.path == '/store/catalog.json' else 404)
                self.end_headers(); self.wfile.write(body)
            def log_message(self, *args): pass
        server = http.server.HTTPServer(('127.0.0.1', 0), Handler)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        self.addCleanup(server.shutdown)
        return 'http://127.0.0.1:%d' % server.server_port

    def run_demo(self, catalog):
        env = dict(self.env, SOJU_DIDIM_STORE=self.serve_catalog(catalog))
        return subprocess.run([sys.executable, str(SCRIPT), 'demo', 'test-game'], env=env, capture_output=True, text=True)

    def test_demo_needs_a_listed_demo(self):
        result = self.run_demo({'schema': 1, 'games': [{'id': 'test-game'}]})
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('no demo yet', result.stderr)

    def test_demo_from_another_host_is_refused(self):
        demo = {'id': 'test-game-demo', 'url': 'https://example.com/demo.zip', 'size': 10, 'sha256': '0' * 64}
        result = self.run_demo({'schema': 1, 'games': [{'id': 'test-game', 'demo': demo}]})
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('cannot use', result.stderr)
        same_id = dict(demo, id='test-game', url='https://github.com/BCD1210/soju/releases/download/x/demo.zip')
        result = self.run_demo({'schema': 1, 'games': [{'id': 'test-game', 'demo': same_id}]})
        self.assertIn('cannot use', result.stderr)

    @unittest.skipUnless(sys.platform == 'darwin', 'needs ditto and codesign')
    def test_download_checks_hash_then_installs(self):
        import hashlib, importlib.util
        archive = make_zip(self.dir, dict(GOOD, id='test-game-demo'))
        data = archive.read_bytes()
        spec = importlib.util.spec_from_file_location('didim_games', SCRIPT)
        module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
        old = os.environ.get('SOJU_BASE'); os.environ['SOJU_BASE'] = str(self.base)
        try:
            with self.assertRaises(ValueError):
                module.download_install('d.zip', len(data), '0' * 64, archive.as_uri(), expect='test-game-demo')
            self.assertFalse((self.base / 'didim-games/test-game-demo').exists())
            module.download_install('d.zip', len(data), hashlib.sha256(data).hexdigest(), archive.as_uri(), expect='test-game-demo')
            self.assertTrue((self.base / 'didim-games/test-game-demo/Test Game.app').is_dir())
        finally:
            if old is None: os.environ.pop('SOJU_BASE', None)
            else: os.environ['SOJU_BASE'] = old

    def test_catalog_copies_match_and_are_valid(self):
        bundled = json.loads((ROOT / 'resources/didim-store.json').read_text())
        public = json.loads((ROOT / 'docs/store/catalog.json').read_text())
        self.assertEqual(bundled, public)
        self.assertEqual(bundled['schema'], 1)
        for game in bundled['games']:
            self.assertTrue(game['checkout'].startswith('https://buy.polar.sh/'))
            self.assertTrue(game['cover'].startswith('https://didimlabs.com/store/'))
            self.assertTrue((ROOT / 'docs/store' / game['cover'].rsplit('/', 1)[1]).is_file())
            if 'demo' in game:
                self.assertTrue(game['demo']['url'].startswith('https://github.com/BCD1210/soju/releases/download/'))
                self.assertNotEqual(game['demo']['id'], game['id'])
