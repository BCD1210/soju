import importlib.util
import json
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('library', ROOT / 'scripts/game-library.py')
library = importlib.util.module_from_spec(spec)
spec.loader.exec_module(library)


def field(n, data):
    def integer(value):
        result = bytearray()
        while value >= 128:
            result.append((value & 127) | 128); value >>= 7
        result.append(value); return bytes(result)
    return integer(n * 8 + 2) + integer(len(data)) + data


class LibraryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="soju library ' ")
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.lib = library.Library(self.base)
        for platform, folder in library.FOLDERS.items():
            self.put(self.base / folder / 'drive_c' / library.CLIENTS[platform], '')

    def put(self, path, data):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data if isinstance(data, bytes) else data.encode())
        return path

    def steam(self, uid='123', title='A game', folder='A game', flags=4, root=None):
        root = root or self.base / 'steam-bottle/drive_c/Program Files (x86)/Steam'
        game = root / 'steamapps/common' / folder
        game.mkdir(parents=True, exist_ok=True)
        manifest = f'"AppState" {{ "appid" "{uid}" "name" "{title}" "StateFlags" "{flags}" "installdir" "{folder}" }}'
        self.put(root / 'steamapps' / ('appmanifest_' + uid + '.acf'), manifest)
        return root, game

    def epic(self, uid='epic-app', **changes):
        item = dict(AppName=uid, DisplayName='An Epic Game', InstallLocation='C:\\Games\\Epic',
                    CatalogNamespace='namespace', CatalogItemId='catalog', LaunchExecutable='Game.exe')
        item.update(changes)
        self.put(self.base / 'epic-bottle/drive_c/Games/Epic/Game.exe', '')
        self.put(self.base / 'epic-bottle/drive_c/ProgramData/Epic/EpicGamesLauncher/Data/Manifests' / (uid + '.item'), json.dumps(item))

    def bnet(self, uid='osi', folder='Diablo'):
        prefix = self.base / 'bottle'
        self.put(prefix / 'drive_c/Games' / folder / '.build.info', 'build')
        record = field(1, uid.encode()) + field(3, field(1, ('C:/Games/' + folder).encode()))
        self.put(prefix / 'drive_c/ProgramData/Battle.net/Agent/product.db', field(1, record))

    def gog(self, uid='77', name='GOG game'):
        path = self.base / 'gog-bottle/drive_c/Custom/GOG game'
        self.put(path / ('goggame-' + uid + '.info'), json.dumps(dict(gameId=uid, rootGameId=uid, name=name)))
        database = self.base / 'gog-bottle/drive_c/ProgramData/GOG.com/Galaxy/storage/galaxy-2.0.db'
        database.parent.mkdir(parents=True, exist_ok=True)
        with sqlite3.connect(database) as db:
            db.execute('CREATE TABLE InstalledBaseProducts (productId INTEGER, installationPath TEXT)')
            db.execute('CREATE TABLE Products (id INTEGER, name TEXT)')
            db.execute('INSERT INTO InstalledBaseProducts VALUES (?, ?)', (uid, 'C:\\Custom\\GOG game'))
            db.execute('INSERT INTO Products VALUES (?, ?)', (uid, name))
        return database

    def test_four_platforms_and_launch_argv(self):
        self.steam(); self.epic(); self.bnet(); database = self.gog()
        before = database.read_bytes()
        result = self.lib.scan()
        self.assertEqual(len(result['games']), 4)
        self.assertEqual(result['warnings'], [])
        self.assertEqual(self.lib.command('steam:123')[-2:], ['steam', 'steam://rungameid/123'])
        self.assertEqual(self.lib.command('battlenet:osi')[-2:], ['battlenet', '--exec=launch OSI'])
        self.assertEqual(self.lib.command('epic:epic-app')[-1], 'com.epicgames.launcher://apps/namespace%3Acatalog%3Aepic-app?action=launch&silent=true')
        self.assertEqual(self.lib.command('gog:77')[-3:], ['/command=runGame', '/gameId=77', '/path=C:\\Custom\\GOG game'])
        self.assertEqual(before, database.read_bytes(), 'Scanning must never modify Galaxy')
        self.assertNotIn('arguments', result['games'][0])

    def test_external_steam_library_and_case_insensitive_path(self):
        root = self.base / 'external/My Games'
        self.steam(root=root)
        prefix = self.base / 'steam-bottle'
        (prefix / 'dosdevices').mkdir()
        (prefix / 'dosdevices/d:').symlink_to(self.base / 'external', target_is_directory=True)
        self.put(prefix / 'drive_c/Program Files (x86)/Steam/steamapps/libraryfolders.vdf',
                 '"libraryfolders" { "0" { "path" "D:\\\\my games" } "1" { "path" "D:\\\\My Games" } }')
        self.assertEqual([g['id'] for g in self.lib.scan()['games']], ['steam:123'])

    def test_invalid_record_does_not_hide_other_platforms(self):
        root, _ = self.steam(); self.epic(); self.bnet()
        self.put(root / 'steamapps/appmanifest_broken.acf', '"AppState" { "appid"')
        self.put(self.base / 'epic-bottle/drive_c/ProgramData/Epic/EpicGamesLauncher/Data/Manifests/broken.item', '{')
        result = self.lib.scan()
        self.assertEqual(len(result['games']), 3)
        self.assertEqual(len(result['warnings']), 2)

    def test_launch_rescans_deleted_installation(self):
        _, game = self.steam()
        self.lib.scan(); game.rmdir()
        with self.assertRaisesRegex(ValueError, 'no longer installed'): self.lib.command('steam:123')

    def test_partial_download_requires_official_launcher(self):
        self.steam(flags=2)
        self.epic(bIsIncompleteInstall=True)
        self.assertEqual(len(self.lib.scan()['games']), 2)
        for key in ['steam:123', 'epic:epic-app']:
            with self.assertRaisesRegex(ValueError, 'Finish'): self.lib.command(key)

    def test_missing_launcher_detected(self):
        self.steam()
        (self.base / 'steam-bottle/drive_c' / library.CLIENTS['steam']).unlink()
        with self.assertRaisesRegex(ValueError, 'Install this platform'): self.lib.command('steam:123')

    def test_unmapped_battlenet_title_is_visible_but_not_guessed(self):
        self.bnet('future', 'Future game')
        result = self.lib.scan()
        self.assertEqual(result['games'][0]['title'], 'Future game')
        with self.assertRaisesRegex(ValueError, 'official launcher'): self.lib.command('battlenet:future')

    def test_gog_fallback_and_dlc_filter(self):
        root = self.base / 'gog-bottle/drive_c/GOG Games/Sample'
        for uid, parent in [('10', '10'), ('11', '10')]:
            self.put(root / ('goggame-' + uid + '.info'), json.dumps(dict(gameId=uid, rootGameId=parent, name='Sample')))
        self.assertEqual([g['id'] for g in self.lib.scan()['games']], ['gog:10'])
        self.epic(MainGameAppName='other-app')
        self.assertEqual(len(self.lib.scan()['games']), 1)

    def test_titles_never_become_commands(self):
        self.steam(title='$(touch SHOULD_NOT_EXIST); `false` & ★')
        result = subprocess.run(['python3', str(ROOT / 'scripts/game-library.py'), 'launch', 'steam:123',
                                 '--dry-run', '--base', str(self.base)], capture_output=True, text=True, check=True)
        command = json.loads(result.stdout)
        self.assertEqual(command[-2:], ['steam', 'steam://rungameid/123'])
        with self.assertRaises(ValueError): self.lib.command('steam:123; echo BAD')
        self.assertFalse((self.base / 'SHOULD_NOT_EXIST').exists())

    def test_path_traversal_and_malformed_binary_rejected(self):
        for path in ['C:\\Games\\..\\Secrets', '\\\\server\\share', '/etc/passwd', 'relative', 'C:\\a\x00b']:
            with self.assertRaises(ValueError): library.windows_path(self.base, path)
        for data in [b'\x0a\xff', b'\x00', b'\x0a\x10abc', b'\xff' * 11]:
            with self.assertRaises(ValueError): library.protobuf(data)
        with self.assertRaises(ValueError): library.vdf('"key" { "bad" }')

    def test_steam_manifest_identity_must_match_filename(self):
        root, _ = self.steam()
        manifest = root / 'steamapps/appmanifest_123.acf'
        manifest.write_text(manifest.read_text().replace('"123"', '"456"'))
        self.assertEqual(self.lib.scan()['games'], [])

    def test_invalid_executable_artwork_is_ignored(self):
        from game_artwork import executable_icon
        broken = self.put(self.base / 'broken.exe', b'MZ' + bytes(100))
        self.assertIsNone(executable_icon(broken, self.base / 'art'))
        self.assertFalse((self.base / 'art').exists())

    def test_gog_successful_handoff_is_not_reported_as_failure(self):
        success = "Initialization strategy returned exit code -1 (Message passed in argument has been sent successfully to another client.)"
        for message, code, expected in [(success, 255, 0), ('Unexpected failure', 255, 255), (success, 1, 1), ('Started', 0, 0)]:
            result = subprocess.run(['python3', str(ROOT / 'scripts/gog-launch.py'),
                                     'python3', '-c', 'import sys; print(sys.argv[1]); sys.exit(int(sys.argv[2]))',
                                     message, str(code)], capture_output=True, text=True)
            self.assertEqual(result.returncode, expected)
            self.assertIn(message, result.stdout)

    def test_offline_external_library_shows_a_warning(self):
        prefix = self.base / 'steam-bottle'
        self.put(prefix / 'drive_c/Program Files (x86)/Steam/steamapps/libraryfolders.vdf',
                 '\"libraryfolders\" { \"0\" { \"path\" \"D:\\\\Games\" } }')
        result = self.lib.scan()
        self.assertEqual(result['games'], [])
        self.assertIn('drive is unavailable', result['warnings'][0])

    def test_no_installations_is_valid_empty_library(self):
        self.assertEqual(self.lib.scan(), {'games': [], 'warnings': []})


if __name__ == '__main__': unittest.main()
