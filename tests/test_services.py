import importlib.util
import json
import os
from pathlib import Path
import sqlite3
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
import library_services as svc
spec = importlib.util.spec_from_file_location('soju_account_scanner', ROOT/'scripts/game-library.py')
scanner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scanner)


class StoreTests(unittest.TestCase):
    def test_steam_prices_and_missing_price(self):
        payload = {'items': [
            {'type': 'app', 'id': 10, 'name': 'Game', 'price': {'final': 0, 'currency': 'USD'}},
            {'type': 'app', 'id': 20, 'name': 'Unpriced'},
            {'type': 'app', 'id': 30, 'name': 'Paid', 'price': {'final': 1999, 'currency': 'USD'}}]}
        with patch.object(svc, 'get_json', return_value=payload):
            rows = svc.steam_search('game', 'US')
        self.assertEqual([x['price'] for x in rows], ['Free', 'View price in store', 'USD 19.99'])
        self.assertEqual(rows[0]['url'], 'https://store.steampowered.com/app/10/')

    def test_gog_currency_and_fixed_store_host(self):
        payload = {'products': [{'id': '10', 'title': 'A game', 'slug': '../../evil',
                    'price': {'finalMoney': {'amount': '14.99', 'currency': 'EUR'}},
                    'coverHorizontal': 'https://evil.example/art.png', 'operatingSystems': ['windows']}]}
        with patch.object(svc, 'get_json', return_value=payload):
            game = svc.gog_search('game', 'DE')[0]
        self.assertEqual(game['price'], 'EUR 14.99')
        self.assertEqual(game['url'], 'https://www.gog.com/game/10')
        self.assertIsNone(game['artwork'])

    def test_artwork_host_validation(self):
        for value in ['file:///etc/passwd', 'https://steamstatic.com.evil.test/x',
                      'https://steamstatic.com@evil.test/x', 'http://shared.steamstatic.com/x',
                      'https://shared.steamstatic.com:8080/x']:
            self.assertIsNone(svc.safe_image(value, 'steam'))
        self.assertEqual(svc.safe_image('https://shared.steamstatic.com/x', 'steam'), 'https://shared.steamstatic.com/x')

    def test_one_store_failure_keeps_other_results(self):
        game = dict(id='steam:10', platform='steam', title='Witcher')
        with patch.object(svc, 'steam_search', return_value=[game]), patch.object(svc, 'gog_search', side_effect=svc.ServiceError('offline')):
            data = svc.search('witcher', 'US')
        self.assertEqual(data['games'], [game])
        self.assertEqual(len(data['warnings']), 1)

    def test_relevance_precedes_alphabetical_order(self):
        games = [dict(id='steam:1', platform='steam', title='An unrelated game'),
                 dict(id='steam:2', platform='steam', title='The Witcher 3')]
        with patch.object(svc, 'steam_search', return_value=games), patch.object(svc, 'gog_search', return_value=[]):
            self.assertEqual(svc.search('witcher', 'US')['games'][0]['id'], 'steam:2')

    def test_invalid_search_does_not_send_request(self):
        with patch.object(svc, 'get_json') as fetch:
            for query, region in [('x', 'US'), ('game', '../')]:
                with self.assertRaises(svc.ServiceError): svc.search(query, region)
            fetch.assert_not_called()

    def test_redirects_are_not_followed(self):
        with self.assertRaises(svc.ServiceError):
            svc.NoRedirect().redirect_request(None, None, 302, '', {}, 'https://other.example/')


class AccountTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.base = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def create_galaxy(self):
        path = self.base/'gog-bottle/drive_c/ProgramData/GOG.com/Galaxy/storage/galaxy-2.0.db'
        path.parent.mkdir(parents=True)
        db = sqlite3.connect(path)
        db.executescript("""
        CREATE TABLE LibraryReleases(id INTEGER,userId INTEGER,releaseKey TEXT);
        CREATE TABLE LicensedReleases(libraryId INTEGER,isOwned INTEGER);
        CREATE TABLE ReleaseProperties(releaseKey TEXT,isDlc INTEGER);
        CREATE TABLE ProductsToReleaseKeys(releaseKey TEXT,gogId INTEGER);
        CREATE TABLE Products(id INTEGER,name TEXT);
        CREATE TABLE GamePieceTypes(id INTEGER,type TEXT);
        CREATE TABLE GamePieces(releaseKey TEXT,gamePieceTypeId INTEGER,value TEXT);
        INSERT INTO GamePieceTypes VALUES(1,'originalTitle');
        INSERT INTO LibraryReleases VALUES(1,111,'gog_10'),(2,111,'gog_20'),(3,111,'gog_30'),(4,222,'gog_40');
        INSERT INTO LicensedReleases VALUES(1,1),(2,0),(3,1),(4,1);
        INSERT INTO ReleaseProperties VALUES('gog_30',1);
        INSERT INTO ProductsToReleaseKeys VALUES('gog_10',10);
        INSERT INTO Products VALUES(10,'Owned Game');
        INSERT INTO GamePieces VALUES('gog_40',1,'{"title":"Another Account Game"}');
        """)
        db.commit(); db.close()
        return path

    def test_gog_requires_account_choice_and_excludes_unowned_dlc(self):
        self.create_galaxy()
        with self.assertRaises(svc.ServiceError): svc.gog_owned(self.base, {})
        games, _ = svc.gog_owned(self.base, {'user_id': '111'})
        self.assertEqual([g['id'] for g in games], ['gog:10'])

    def test_gog_uninstalled_game_metadata_and_readonly(self):
        path = self.create_galaxy()
        before = path.read_bytes()
        games, _ = svc.gog_owned(self.base, {'user_id': '222'})
        self.assertEqual(games[0]['title'], 'Another Account Game')
        self.assertEqual(path.read_bytes(), before)

    def test_steam_snapshot_valid_empty_and_incomplete(self):
        value = dict(state='ready', steamid='76561198000000000', game_count=1, games=[dict(appid=10, name='Owned', token='ignored')])
        games, label = svc.steam_owned({'snapshot': json.dumps(value)})
        self.assertEqual(games, [dict(id='steam:10', platform='steam', title='Owned')])
        self.assertNotIn(value['steamid'], label)
        value.update(game_count=0, games=[])
        self.assertEqual(svc.steam_owned({'snapshot': json.dumps(value)})[0], [])
        for bad in [dict(value, state='sign-in'), dict(value, game_count=1), dict(value, steamid='invalid'), dict(value, game_count=True)]:
            with self.assertRaises(svc.ServiceError):
                svc.steam_owned({'snapshot': json.dumps(bad)})

    def test_steam_snapshot_rejects_duplicate_or_unsafe_ids(self):
        for games in [[dict(appid=10, name='A'), dict(appid=10, name='A')], [dict(appid='../10', name='A')], [dict(appid=10, name='')]]:
            value = dict(state='ready', steamid='76561198000000000', game_count=len(games), games=games)
            with self.assertRaises(svc.ServiceError):
                svc.steam_owned({'snapshot': json.dumps(value)})

    def test_steam_snapshot_cli_accepts_libraries_larger_than_pipe_buffer(self):
        value = dict(state='ready', steamid='76561198000000000', game_count=2000,
                     games=[dict(appid=i + 1, name='Owned game ' + str(i)) for i in range(2000)])
        result = subprocess.run([sys.executable, str(ROOT/'scripts/library-service.py'), 'sync', 'steam', '--base', str(self.base)],
                                input=json.dumps({'snapshot': json.dumps(value)}), text=True, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertEqual(json.loads(result.stdout)['count'], 2000)
        self.assertEqual(len(svc.saved_accounts(self.base)[1]), 2000)

    def test_failed_sync_does_not_disclose_settings_or_replace_cache(self):
        svc.save_account(self.base, 'steam', [{'id': 'steam:10', 'title': 'Kept'}], 'Steam')
        path = svc.account_path(self.base, 'steam')
        before = path.read_bytes()
        secret = 'THIS_VALUE_MUST_NOT_APPEAR'
        result = subprocess.run([sys.executable, str(ROOT/'scripts/library-service.py'), 'sync', 'steam', '--base', str(self.base)],
                                input=json.dumps({'snapshot': secret}),
                                text=True, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn(secret, result.stdout + result.stderr)
        self.assertEqual(before, path.read_bytes())

    def test_account_cache_is_private_and_rejects_commands(self):
        svc.save_account(self.base, 'steam', [{'id': 'steam:10', 'title': 'Owned', 'arguments': ['sh', '-c', 'bad']}], 'Steam')
        self.assertEqual(stat.S_IMODE(svc.account_path(self.base, 'steam').stat().st_mode), 0o600)
        accounts, games, warnings = svc.saved_accounts(self.base)
        self.assertEqual(accounts[0]['count'], 1)
        self.assertFalse(warnings)
        self.assertNotIn('arguments', games[0])
        self.assertEqual(games[0]['install_path'], '')

    def test_imported_game_cannot_play_until_installed(self):
        svc.save_account(self.base, 'steam', [{'id': 'steam:10', 'title': 'Owned'}], 'Steam')
        lib = scanner.Library(self.base)
        self.assertEqual(lib.scan()['games'][0]['title'], 'Owned')
        with self.assertRaisesRegex(ValueError, 'not installed'): lib.command('steam:10')
        with self.assertRaisesRegex(ValueError, 'Install this launcher'): lib.command('steam:10', install=True)
        client = self.base/'steam-bottle/drive_c/Program Files (x86)/Steam/steam.exe'
        client.parent.mkdir(parents=True); client.touch()
        self.assertEqual(lib.command('steam:10', install=True)[-1], 'steam://install/10')

    def test_installed_manifest_wins_over_owned_snapshot(self):
        client = self.base/'steam-bottle/drive_c/Program Files (x86)/Steam'
        (client/'steamapps/common/Owned').mkdir(parents=True)
        (client/'steam.exe').touch()
        (client/'steamapps/appmanifest_10.acf').write_text('"AppState" {"appid" "10" "name" "Installed title" "installdir" "Owned" "StateFlags" "4"}')
        svc.save_account(self.base, 'steam', [{'id': 'steam:10', 'title': 'Old title'}], 'Steam')
        data = scanner.Library(self.base).scan()
        self.assertEqual(len(data['games']), 1)
        self.assertEqual(data['games'][0]['title'], 'Installed title')
        self.assertTrue(data['games'][0]['install_path'])
        self.assertTrue(data['games'][0]['owned'])

    def test_malformed_import_is_isolated(self):
        svc.save_account(self.base, 'steam', [{'id': 'steam:10', 'title': 'Kept'}], 'Steam')
        svc.account_path(self.base, 'gog').write_text('{"schema":1,"platform":"gog","games":[{"id":"gog:../../evil","title":"Bad"}]}')
        _, games, warnings = svc.saved_accounts(self.base)
        self.assertEqual([g['id'] for g in games], ['steam:10'])
        self.assertEqual(len(warnings), 1)


if __name__ == '__main__':
    unittest.main()
