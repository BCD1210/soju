"""Xbox PC / Microsoft Store through the Xodus backend: search, import, downloads, build recipe.

No network, Keychain or real backend: every Xodus call is a fake script or a mock.
"""
import datetime
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
import library_services as svc


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


library = load('soju_xbox_library', 'game-library.py')
download = load('soju_xbox_download', 'xbox-download.py')
backend = load('soju_xodus_backend', 'fetch-xodus-backend.py')
runner = load('soju_xbox_run', 'xbox-run.py')
prefix = load('soju_xbox_prefix', 'xbox-prefix.py')
runtime = load('soju_xbox_runtime', 'fetch-xbox-runtime.py')

SILKSONG, VOID, PASS = '9N116V0599HB', '9MW9469V91LM', '9NBLGGH4R315'
FUTURE = (datetime.datetime.now(datetime.timezone.utc) + datetime.timedelta(days=30)).isoformat()
PAST = '2020-01-01T00:00:00Z'


def product(uid, title, kind='Game', fmt='MSIXVC', price=19.99, buy=True):
    return {'ProductId': uid, 'ProductType': kind,
            'LocalizedProperties': [{'ProductTitle': title, 'Images': [
                {'ImagePurpose': 'BoxArt', 'Uri': '//store-images.s-microsoft.com/image/apps.1.' + uid}]}],
            'DisplaySkuAvailabilities': [{'Sku': {'Properties': {'IsTrial': False, 'Packages': [
                {'PackageFormat': fmt, 'PlatformDependencies': [{'PlatformName': 'Windows.Desktop'}]},
                {'PackageFormat': 'XVC', 'PlatformDependencies': [{'PlatformName': 'Windows.Xbox'}]}]}},
                'Availabilities': [{'Actions': ['Details', 'Purchase'] if buy else ['Details'],
                                    'OrderManagementData': {'Price': {'ListPrice': price, 'CurrencyCode': 'USD'}}}]}]}


class XboxStoreTests(unittest.TestCase):
    def test_search_uses_public_catalog_prices_and_xbox_links(self):
        suggest = {'Results': [{'Products': [{'ProductId': SILKSONG}, {'ProductId': VOID}, {'ProductId': 'bad id'}]}]}
        catalog = {'Products': [product(SILKSONG, 'Hollow Knight: Silksong'),
                                product(VOID, 'Old edition', price=0, buy=False)]}
        with patch.object(svc, 'get_json', side_effect=[suggest, catalog]) as get:
            rows = svc.xbox_search('hollow', 'KR')
        self.assertIn('productFamilies/autosuggest', get.call_args_list[0].args[0])
        self.assertEqual([r['id'] for r in rows], ['xbox:' + SILKSONG, 'xbox:' + VOID])
        self.assertEqual(rows[0]['price'], 'USD 19.99')
        # A delisted game keeps a 0 list price but cannot be bought.
        self.assertEqual(rows[1]['price'], 'View price in store')
        self.assertEqual(rows[0]['url'], 'https://www.xbox.com/ko-KR/games/store/-/' + SILKSONG)
        self.assertTrue(rows[0]['artwork'].startswith('https://store-images.s-microsoft.com/'))
        self.assertTrue(rows[0]['windows'])

    def test_editions_without_own_packages_are_downloadable_bundles(self):
        bundle = product('9P1HX37NMJLT', 'Deluxe Edition')
        props = bundle['DisplaySkuAvailabilities'][0]['Sku']['Properties']
        props['Packages'] = []
        props['BundledSkus'] = [{'BigId': '9NKX70BBCDRN'}]
        with patch.object(svc, 'get_json', return_value={'Products': [bundle]}):
            meta = svc.xbox_catalog(['9P1HX37NMJLT'], 'US')['9P1HX37NMJLT']
        self.assertEqual((meta['format'], meta['windows']), ('BUNDLE', True))
        with tempfile.TemporaryDirectory() as temp:
            svc.save_account(temp, 'xbox', [dict(id='xbox:9P1HX37NMJLT', platform='xbox', title='Deluxe', format='BUNDLE')], 'Xbox')
            self.assertIsNone(svc.saved_accounts(temp)[1][0]['issue'])

    def test_artwork_must_come_from_microsoft_image_host(self):
        self.assertIsNone(svc.xbox_image('https://evil.test/x.png'))
        self.assertIsNone(svc.xbox_image('//store-images.s-microsoft.com.evil.test/x'))
        self.assertEqual(svc.xbox_image('//store-images.s-microsoft.com/image/a?w=9'),
                         'https://store-images.s-microsoft.com/image/a?w=400')

    def test_three_store_failure_isolation(self):
        game = dict(id='steam:1', platform='steam', title='Hollow')
        with patch.object(svc, 'steam_search', return_value=[game]), patch.object(svc, 'gog_search', return_value=[]), \
                patch.object(svc, 'xbox_search', side_effect=svc.ServiceError('offline')):
            data = svc.search('hollow', 'US')
        self.assertEqual(data['games'], [game])
        self.assertEqual(data['warnings'], ['Microsoft Store search is unavailable. Please retry later.'])


class XboxImportTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        cli = self.base / 'xodus/bin/xodus-cli'
        cli.parent.mkdir(parents=True)
        cli.write_text('#!/bin/sh\nexit 0\n')
        cli.chmod(0o755)

    def fake(self, library, stderr='', code=0):
        def run(args, **kwargs):
            self.assertNotIn('shell', kwargs)
            self.assertEqual(kwargs['stdin'], subprocess.DEVNULL)
            if args[1] == 'profile':
                return subprocess.CompletedProcess(args, 0, json.dumps({'schema': 1, 'account': 'me@example.test', 'gamertag': 'Tester'}), '')
            return subprocess.CompletedProcess(args, code, json.dumps(library), stderr)
        return run

    def entitlements(self):
        row = dict(status='Active', isTrial=False, sharingSource='None', endDate=FUTURE, satisfiedByProductId=None, special=False)
        return {'schema': 1, 'account': 'me@example.test', 'market': 'US', 'checkedAt': FUTURE, 'entitlements': [
            dict(row, productId=SILKSONG),
            dict(row, productId=VOID, satisfiedByProductId='CFQ7TTC0KGQ8'),
            dict(row, productId='9PASTPASTPAS', endDate=PAST),
            dict(row, productId='9INACTIVEXXX', status='Expired'),
            dict(row, productId='9SPECIALXXXX', special=True),
            dict(row, productId='lowercase-id'),
            dict(row, productId=PASS)]}

    def test_owned_and_game_pass_games_are_imported_without_the_account_name(self):
        meta = {SILKSONG: dict(title='Silksong', type='Game', windows=True, format='MSIXVC', artwork=None, price=''),
                VOID: dict(title='Voidheart', type='Game', windows=True, format='MSIXVC2', artwork=None, price=''),
                PASS: dict(title='A subscription', type='Pass', windows=False, format=None, artwork=None, price='')}
        with patch.object(svc.subprocess, 'run', side_effect=self.fake(self.entitlements())), \
                patch.object(svc, 'xbox_catalog', return_value=meta) as catalog:
            games, label = svc.xbox_owned(self.base, {'market': 'US'})
        self.assertEqual(sorted(catalog.call_args.args[0]), sorted([SILKSONG, VOID, PASS]))
        self.assertEqual(label, 'Xbox · Tester')
        self.assertEqual({g['id']: g['source'] for g in games}, {'xbox:' + SILKSONG: 'owned', 'xbox:' + VOID: 'gamepass'})
        svc.save_account(self.base, 'xbox', games, label, 'US')
        saved = (self.base / 'account-libraries/xbox.json').read_text()
        self.assertNotIn('example.test', saved)
        self.assertEqual(json.loads(saved)['market'], 'US')
        accounts, owned, warnings = svc.saved_accounts(self.base)
        self.assertEqual(warnings, [])
        issues = {g['id']: g['issue'] for g in owned}
        self.assertIsNone(issues['xbox:' + SILKSONG])
        self.assertIn('newer package format', issues['xbox:' + VOID])

    def test_backend_failure_hides_its_output(self):
        secret = 'eyJhbGciOi.secret-token'
        with patch.object(svc.subprocess, 'run', side_effect=self.fake({}, stderr=secret, code=1)):
            with self.assertRaises(svc.ServiceError) as error:
                svc.xbox_owned(self.base, {'market': 'US'})
        self.assertNotIn(secret, str(error.exception))
        with patch.object(svc.subprocess, 'run', side_effect=self.fake({}, stderr='NEXUS_CREDENTIAL_ERROR_V1 missing ' + secret, code=1)):
            with self.assertRaises(svc.ServiceError) as error:
                svc.xbox_owned(self.base, {'market': 'US'})
        self.assertIn('Sign in', str(error.exception))
        self.assertNotIn(secret, str(error.exception))

    def test_missing_backend_and_bad_market_are_reported(self):
        with self.assertRaises(svc.ServiceError):
            svc.xbox_owned(self.base, {'market': 'XX'})
        (self.base / 'xodus/bin/xodus-cli').unlink()
        with self.assertRaises(svc.ServiceError) as error:
            svc.xbox_owned(self.base, {'market': 'US'})
        self.assertIn('Platforms', str(error.exception))

    def test_saved_art_must_be_soju_art_for_that_game(self):
        art = self.base / 'library-artwork'
        art.mkdir()
        (art / ('xbox-' + SILKSONG + '.jpg')).write_bytes(b'\xff\xd8')
        outside = self.base / 'elsewhere.jpg'
        outside.write_bytes(b'\xff\xd8')
        svc.save_account(self.base, 'xbox', [
            dict(id='xbox:' + SILKSONG, platform='xbox', title='A', format='MSIXVC', source='owned', artwork=str(art / ('xbox-' + SILKSONG + '.jpg'))),
            dict(id='xbox:' + VOID, platform='xbox', title='B', format='MSIXVC', source='owned', artwork=str(outside))], 'Xbox')
        _, owned, _ = svc.saved_accounts(self.base)
        art_by_id = {g['id']: g['artwork'] for g in owned}
        self.assertTrue(art_by_id['xbox:' + SILKSONG].endswith('xbox-' + SILKSONG + '.jpg'))
        self.assertIsNone(art_by_id['xbox:' + VOID])

    def test_invalid_saved_xbox_id_rejects_the_import(self):
        svc.save_account(self.base, 'xbox', [dict(id='xbox:9n116v0599hb', platform='xbox', title='A')], 'Xbox')
        _, owned, warnings = svc.saved_accounts(self.base)
        self.assertEqual(owned, [])
        self.assertEqual(warnings, ['Xbox account import could not be read. Reconnect it in Accounts.'])


class XboxLibraryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.games = self.base / 'xbox-bottle/drive_c/XboxGames'
        marker = self.base / 'xbox-bottle/drive_c' / library.CLIENTS['xbox']
        marker.parent.mkdir(parents=True)
        marker.write_text('{}')
        self.lib = library.Library(self.base)

    def downloaded(self, uid=SILKSONG, title='Silksong'):
        folder = self.games / uid
        folder.mkdir(parents=True)
        (folder / '.soju-xbox-install.json').write_text(json.dumps(dict(schema=1, product_id=uid, title=title)))
        return folder

    def engine(self, gptk=True):
        wine = self.base / 'cx26-engine/bin/wine'
        wine.parent.mkdir(parents=True)
        wine.write_text('#!/bin/sh\n')
        wine.chmod(0o755)
        if gptk:
            art = self.base / 'cx26-engine/lib/external/libd3dshared.dylib'
            art.parent.mkdir(parents=True)
            art.write_text('')

    def test_downloaded_game_plays_through_xbox_script(self):
        folder = self.downloaded()
        self.engine()
        game = {g['id']: g for g in self.lib.scan()['games']}['xbox:' + SILKSONG]
        self.assertTrue(game['downloaded'])
        self.assertEqual(game['install_path'], str(folder))
        self.assertIsNone(game['issue'])
        self.assertEqual(self.lib.command('xbox:' + SILKSONG), ['/bin/bash', str(ROOT / 'scripts/xbox.sh'), 'run', SILKSONG])
        with self.assertRaisesRegex(ValueError, 'already downloaded'):
            self.lib.command('xbox:' + SILKSONG, install=True)

    def test_play_needs_the_engine_and_gptk(self):
        self.downloaded()
        with self.assertRaisesRegex(ValueError, 'installs the engine'):
            self.lib.command('xbox:' + SILKSONG)
        self.engine(gptk=False)
        with self.assertRaisesRegex(ValueError, 'Game Porting Toolkit'):
            self.lib.command('xbox:' + SILKSONG)

    def test_owned_game_downloads_through_xbox_script(self):
        svc.save_account(self.base, 'xbox', [dict(id='xbox:' + SILKSONG, platform='xbox', title='Silksong', format='MSIXVC')], 'Xbox')
        command = self.lib.command('xbox:' + SILKSONG, install=True)
        self.assertEqual(command[1:], [str(ROOT / 'scripts/xbox.sh'), 'download', SILKSONG])

    def test_msixvc2_game_cannot_start_a_download(self):
        svc.save_account(self.base, 'xbox', [dict(id='xbox:' + VOID, platform='xbox', title='Void', format='MSIXVC2')], 'Xbox')
        with self.assertRaisesRegex(ValueError, 'newer package format'):
            self.lib.command('xbox:' + VOID, install=True)

    def test_unfinished_download_can_resume_and_takes_the_store_name(self):
        folder = self.games / SILKSONG
        folder.mkdir(parents=True)
        (folder / '.nexus-streaming-v1.jsonl').write_text('{}\n')
        svc.save_account(self.base, 'xbox', [dict(id='xbox:' + SILKSONG, platform='xbox', title='Silksong', format='MSIXVC')], 'Xbox')
        game = {g['id']: g for g in self.lib.scan()['games']}['xbox:' + SILKSONG]
        self.assertFalse(game['downloaded'])
        self.assertEqual(game['title'], 'Silksong')
        self.assertIn('resume', game['issue'])
        self.assertEqual(self.lib.command('xbox:' + SILKSONG, install=True)[-1], SILKSONG)

    def test_mismatched_or_foreign_folders_are_ignored(self):
        folder = self.downloaded()
        (folder / '.soju-xbox-install.json').write_text(json.dumps(dict(schema=1, product_id=VOID, title='x')))
        (self.games / 'not-a-store-id').mkdir()
        result = self.lib.scan()
        self.assertEqual(result['games'], [])
        self.assertTrue(any(w.startswith('xbox:') for w in result['warnings']))

    def test_missing_setup_blocks_download(self):
        (self.base / 'xbox-bottle/drive_c' / library.CLIENTS['xbox']).unlink()
        svc.save_account(self.base, 'xbox', [dict(id='xbox:' + SILKSONG, platform='xbox', title='Silksong', format='MSIXVC')], 'Xbox')
        with self.assertRaisesRegex(ValueError, 'Platforms'):
            self.lib.command('xbox:' + SILKSONG, install=True)


class XboxLocationTests(unittest.TestCase):
    """Downloads on external drives: scripts/xbox_locations.py and its callers."""
    def setUp(self):
        import xbox_locations
        self.locs = xbox_locations
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name).resolve() / 'base'
        self.games = self.base / 'xbox-bottle/drive_c/XboxGames'
        marker = self.base / 'xbox-bottle/drive_c' / library.CLIENTS['xbox']
        marker.parent.mkdir(parents=True)
        marker.write_text('{}')
        self.lib = library.Library(self.base)
        for name, value in (('BASE', self.base), ('DEFAULT', self.games), ('STATE', self.base / 'xbox-locations.json')):
            patcher = patch.object(self.locs, name, value)
            patcher.start()
            self.addCleanup(patcher.stop)
        self.ext = self.base / 'drive/XboxGames'
        self.ext.mkdir(parents=True)
        fs = patch.object(self.locs, 'fs_type', return_value='apfs')
        self.fs = fs.start()
        self.addCleanup(fs.stop)

    def finished(self, root, uid=SILKSONG, title='Silksong'):
        folder = root / uid
        folder.mkdir(parents=True, exist_ok=True)
        (folder / '.soju-xbox-install.json').write_text(json.dumps(dict(schema=1, product_id=uid, title=title)))
        return folder

    def scan(self):
        return {g['id']: g for g in self.lib.scan()['games']}

    def test_location_must_be_a_real_folder_outside_system_folders(self):
        link = self.base / 'to-etc'
        link.symlink_to('/etc')
        for bad in ('', 'relative/folder', str(self.ext) + '/../x', '/', '/Volumes', '/System/Library', '/usr', str(link),
                    str(self.base / 'missing'), 'a\x00b'):
            with self.subTest(bad=bad), self.assertRaises(self.locs.LocationError):
                self.locs.set_current(bad)
        self.assertFalse(self.locs.STATE.exists())
        self.assertEqual(self.locs.set_current(str(self.ext)), self.ext)
        state = json.loads(self.locs.STATE.read_text())
        self.assertEqual(state['current'], str(self.ext))
        self.assertEqual(self.locs.STATE.stat().st_mode & 0o777, 0o600)
        self.assertTrue((self.ext / '.soju-xbox-root.json').is_file())

    def test_unsupported_filesystems_are_refused(self):
        for kind in ('msdos', 'ntfs', 'smbfs', 'nfs', ''):
            self.fs.return_value = kind
            with self.subTest(kind=kind), self.assertRaisesRegex(self.locs.LocationError, 'APFS'):
                self.locs.set_current(str(self.ext))
        for kind in ('hfs', 'exfat'):
            self.fs.return_value = kind
            self.locs.set_current(str(self.ext))

    def test_external_game_is_listed_played_and_downloaded_there(self):
        self.locs.set_current(str(self.ext))
        folder = self.finished(self.ext)
        wine = self.base / 'cx26-engine/bin/wine'
        wine.parent.mkdir(parents=True)
        wine.write_text('#!/bin/sh\n')
        wine.chmod(0o755)
        (self.base / 'cx26-engine/lib/external').mkdir(parents=True)
        (self.base / 'cx26-engine/lib/external/libd3dshared.dylib').write_text('')
        game = self.scan()['xbox:' + SILKSONG]
        self.assertEqual(game['install_path'], str(folder))
        self.assertTrue(game['downloaded'] and game['available'])
        self.assertEqual(self.lib.command('xbox:' + SILKSONG)[-2:], ['run', SILKSONG])
        self.assertEqual(self.locs.locate(SILKSONG), (folder, 'complete'))
        self.assertEqual(self.locs.destination(VOID), self.ext / VOID)
        self.assertEqual(self.lib.scan()['xbox_location']['default'], False)

    def test_partial_download_resumes_in_its_own_folder_after_location_change(self):
        partial = self.games / SILKSONG
        partial.mkdir(parents=True)
        (partial / '.nexus-streaming-v1.jsonl').write_text('{}\n')
        self.locs.set_current(str(self.ext))
        self.assertEqual(self.locs.destination(SILKSONG), partial)
        self.assertFalse((self.ext / SILKSONG).exists())

    def test_unplugged_drive_keeps_the_game_and_never_downloads_to_the_mac(self):
        self.locs.set_current(str(self.ext))
        self.finished(self.ext)
        svc.save_account(self.base, 'xbox', [dict(id='xbox:' + SILKSONG, platform='xbox', title='Silksong', format='MSIXVC')], 'Xbox')
        self.scan()
        unplugged = self.base / 'unplugged'
        self.ext.rename(unplugged)
        game = self.scan()['xbox:' + SILKSONG]
        self.assertEqual((game['downloaded'], game['available']), (True, False))
        self.assertIn('not connected', game['issue'])
        for install in (True, False):
            with self.assertRaisesRegex(ValueError, 'not connected'):
                self.lib.command('xbox:' + SILKSONG, install=install)
        with self.assertRaisesRegex(self.locs.LocationError, 'not connected'):
            self.locs.destination(SILKSONG)
        with self.assertRaisesRegex(self.locs.LocationError, 'not connected'):
            self.locs.destination(VOID)
        self.assertFalse((self.games / SILKSONG).exists())
        self.assertIn(SILKSONG, json.loads(self.locs.STATE.read_text())['games'])
        unplugged.rename(self.ext)
        self.assertTrue(self.scan()['xbox:' + SILKSONG]['available'])

    def test_deleted_game_on_a_connected_drive_is_forgotten(self):
        self.locs.set_current(str(self.ext))
        folder = self.finished(self.ext)
        self.scan()
        (folder / '.soju-xbox-install.json').unlink()
        folder.rmdir()
        self.assertNotIn('xbox:' + SILKSONG, self.scan())
        self.assertNotIn(SILKSONG, json.loads(self.locs.STATE.read_text())['games'])

    def test_renamed_volume_is_found_by_its_id(self):
        volumes = self.base / 'Volumes'
        drive = volumes / 'T7' / 'XboxGames'
        drive.mkdir(parents=True)
        mounted = [(str(volumes / 'T7 1'), 'apfs', []), (str(volumes / 'Share'), 'smbfs', [])]
        with patch.object(self.locs, 'VOLUMES', volumes), patch.object(self.locs, 'mounts', return_value=mounted):
            self.locs.set_current(str(drive))
            self.finished(drive)
            self.scan()
            (volumes / 'T7').rename(volumes / 'T7 1')
            result = self.lib.scan()
            game = {g['id']: g for g in result['games']}['xbox:' + SILKSONG]
            self.assertTrue(game['available'])
            self.assertEqual(game['install_path'], str(volumes / 'T7 1/XboxGames' / SILKSONG))
            self.assertEqual(json.loads(self.locs.STATE.read_text())['current'], str(volumes / 'T7 1/XboxGames'))

    def test_another_drive_with_the_same_name_is_not_used(self):
        self.locs.set_current(str(self.ext))
        (self.ext / '.soju-xbox-root.json').write_text(json.dumps({'schema': 1, 'id': 'someone-else'}))
        with self.assertRaisesRegex(self.locs.LocationError, 'not connected'):
            self.locs.destination(SILKSONG)

    def test_a_second_drive_with_the_same_path_keeps_the_first_drives_games(self):
        self.locs.set_current(str(self.ext))
        self.finished(self.ext)
        self.scan()
        first = self.base / 'drive-a'
        self.ext.parent.rename(first)                    # drive A unplugged
        self.ext.mkdir(parents=True)                     # drive B mounts at the same path
        self.locs.set_current(str(self.ext))
        game = self.scan()['xbox:' + SILKSONG]
        self.assertEqual((game['downloaded'], game['available']), (True, False))
        with self.assertRaisesRegex(ValueError, 'not connected'):
            self.lib.command('xbox:' + SILKSONG, install=True)
        self.assertFalse((self.ext / SILKSONG).exists())
        self.assertEqual(len(json.loads(self.locs.STATE.read_text())['roots']), 2)

    def test_an_unreadable_drive_does_not_forget_its_games(self):
        self.locs.set_current(str(self.ext))
        self.finished(self.ext)
        self.scan()
        real = Path.iterdir
        def broken(path):
            if Path(path) == self.ext:
                raise OSError('I/O error')
            return real(path)
        with patch.object(Path, 'iterdir', broken):
            result = self.lib.scan()
        self.assertTrue(any('could not be read' in w for w in result['warnings']))
        self.assertIn(SILKSONG, json.loads(self.locs.STATE.read_text())['games'])

    def test_game_in_two_folders_uses_the_finished_one(self):
        self.locs.set_current(str(self.ext))
        partial = self.games / SILKSONG
        partial.mkdir(parents=True)
        (partial / '.nexus-streaming-v1.jsonl').write_text('{}\n')
        done = self.finished(self.ext)
        self.assertEqual(self.locs.locate(SILKSONG), (done, 'complete'))

    def test_links_out_of_a_download_folder_are_ignored(self):
        self.locs.set_current(str(self.ext))
        outside = self.finished(self.base / 'elsewhere')
        (self.ext / VOID).symlink_to(outside)
        self.assertEqual(self.locs.locate(VOID), (None, None))
        with self.assertRaisesRegex(self.locs.LocationError, 'link'):
            self.locs.destination(VOID)

    def fake_download(self, streaming):
        cli = self.base / 'xodus-cli'
        plan = json.dumps(dict(schema=1, status='ready', choices=[XboxDownloadTests.CHOICE]))
        cli.write_text('#!/bin/sh\n[ "$1" = resolve-package ] && { printf "%s\\n" ' + repr(plan) + '; exit 0; }\n'
                       '[ "$1" = streaming ] || exit 9\n' + streaming)
        cli.chmod(0o755)
        env = dict(os.environ, SOJU_BASE=str(self.base), SOJU_XODUS_BIN=str(cli))
        return subprocess.run([sys.executable, str(ROOT / 'scripts/xbox-download.py'), SILKSONG],
                              env=env, capture_output=True, text=True, timeout=30)

    def test_download_goes_to_the_chosen_drive_and_is_remembered(self):
        self.locs.set_current(str(self.ext))
        done = "printf '%s\\n' " + repr('NEXUS_PROGRESS ' + json.dumps(dict(schema=1, phase='complete'))) + '\n'
        result = self.fake_download(done)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((self.ext / SILKSONG / '.soju-xbox-install.json').is_file())
        self.assertFalse((self.games / SILKSONG).exists())
        self.assertEqual(json.loads(self.locs.STATE.read_text())['games'][SILKSONG]['root'], str(self.ext))

    def test_out_of_space_is_reported_in_words(self):
        line = 'echo "Error: not enough free disk space on /Volumes/T7/x: need 5000000000 bytes, have 1000000000 bytes"; exit 1\n'
        result = self.fake_download(line)
        self.assertEqual(result.returncode, 1)
        self.assertIn('Not enough free space', result.stderr)
        self.assertIn('4.0 GB', result.stderr)
        self.assertNotIn('/Volumes/T7/x', result.stdout + result.stderr)

    def test_tampered_saved_location_is_refused(self):
        self.locs.save({'schema': 1, 'current': '/usr', 'roots': [{'path': '/usr', 'id': ''}], 'games': {}})
        with self.assertRaisesRegex(self.locs.LocationError, 'not allowed'):
            self.locs.destination(SILKSONG)

    def test_paths_in_messages_hide_the_user_name(self):
        self.assertEqual(self.locs.tilde(Path.home() / 'Games'), '~/Games')
        self.assertEqual(self.locs.tilde('/Volumes/T7/XboxGames'), '/Volumes/T7/XboxGames')

    def test_cli_sets_and_resets_the_location(self):
        env = dict(os.environ, SOJU_BASE=str(self.base))
        run = lambda *a: subprocess.run(['/bin/bash', str(ROOT / 'scripts/xbox.sh'), 'location', *a],
                                        env=env, capture_output=True, text=True, timeout=30)
        bad = run('/System')
        self.assertEqual(bad.returncode, 1)
        self.assertIn('external drive', bad.stderr)
        shown = run()
        self.assertEqual(shown.returncode, 0, shown.stderr)
        self.assertIn('Download location', shown.stdout)
        self.assertEqual(run('--default').returncode, 0)
        action = subprocess.run(['/bin/bash', str(ROOT / 'scripts/desktop-action.sh'), 'xbox', 'location', '/System'],
                                env=env, capture_output=True, text=True, timeout=30)
        self.assertNotEqual(action.returncode, 0)


class XboxDownloadTests(unittest.TestCase):
    CHOICE = dict(contentId='807c7d6a-409f-48be-8190-30b09baf7cd4', kind='Game', packageFormat='MSIXVC',
                  platform='Windows.Desktop', productId=SILKSONG, supported=True, title='Silksong')

    def plan(self, status, *choices):
        return dict(schema=1, status=status, choices=list(choices))

    def test_package_choice(self):
        console = dict(self.CHOICE, platform='Windows.Xbox', packageFormat='XVC', supported=False, contentId='b6efa75d-2a92-4de8-bfc6-5d61787fa333')
        self.assertEqual(download.choose(self.plan('ready', self.CHOICE, console), SILKSONG), self.CHOICE)
        self.assertEqual(download.choose(self.plan('choice', self.CHOICE, console), SILKSONG), self.CHOICE)
        other = dict(self.CHOICE, contentId='11111111-1111-4111-8111-111111111111', title='Other')
        with self.assertRaisesRegex(download.DownloadError, 'several games'):
            download.choose(self.plan('choice', self.CHOICE, other), SILKSONG)
        with self.assertRaisesRegex(download.DownloadError, 'newer package format'):
            download.choose(self.plan('unavailable', dict(self.CHOICE, packageFormat='MSIXVC2', supported=False)), SILKSONG)
        with self.assertRaises(download.DownloadError):
            download.choose(self.plan('ready', dict(self.CHOICE, contentId='--evil')), SILKSONG)
        with self.assertRaises(download.DownloadError):
            download.choose({'schema': 2}, SILKSONG)
        with self.assertRaises(download.DownloadError):
            download.product_id('../etc')
        self.assertEqual(download.product_id('xbox:' + SILKSONG.lower()), SILKSONG)

    def run_download(self, phase):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            cli = base / 'xodus-cli'
            progress = [dict(schema=1, event='progress', sequence=0, phase='initializing'),
                        dict(schema=1, event='progress', sequence=1, phase='downloading', completedBytes=50, totalBytes=100,
                             bytesPerSecond=10, paused=False, url='https://secret.example/token'),
                        dict(schema=1, event='progress', sequence=2, phase=phase, completedBytes=100, totalBytes=100)]
            lines = ''.join("printf '%s\\n' " + repr('NEXUS_PROGRESS ' + json.dumps(p)) + '\n' for p in progress)
            plan = json.dumps(self.plan('ready', self.CHOICE))
            cli.write_text('#!/bin/sh\n[ "$1" = resolve-package ] && { printf "%s\\n" ' + repr(plan) + '; exit 0; }\n'
                           '[ "$1" = streaming ] || exit 9\n' + lines + 'echo plain backend line\nexit 0\n')
            cli.chmod(0o755)
            svc.save_account(base, 'xbox', [dict(id='xbox:' + SILKSONG, platform='xbox', title='Silksong Deluxe')], 'Xbox', 'KR')
            env = dict(os.environ, SOJU_BASE=str(base), SOJU_XODUS_BIN=str(cli))
            result = subprocess.run([sys.executable, str(ROOT / 'scripts/xbox-download.py'), SILKSONG],
                                    env=env, capture_output=True, text=True, timeout=30)
            marker = base / 'xbox-bottle/drive_c/XboxGames' / SILKSONG / '.soju-xbox-install.json'
            return result, (json.loads(marker.read_text()) if marker.exists() else None)

    def test_marker_only_after_a_complete_download(self):
        result, marker = self.run_download('complete')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(marker['product_id'], SILKSONG)
        self.assertEqual(marker['content_id'], self.CHOICE['contentId'])
        self.assertEqual(marker['title'], 'Silksong Deluxe')
        self.assertIn('50% · 50 B / 100 B', result.stdout)
        self.assertIn('plain backend line', result.stdout)
        self.assertNotIn('NEXUS_PROGRESS', result.stdout)
        self.assertNotIn('secret.example', result.stdout)
        result, marker = self.run_download('failed')
        self.assertEqual(result.returncode, 1)
        self.assertIsNone(marker)
        self.assertIn('resume', result.stderr)


class XodusBackendRecipeTests(unittest.TestCase):
    def setUp(self):
        self.manifest = json.loads(backend.MANIFEST.read_text())

    def test_manifest_pins_source_toolchain_and_every_patch(self):
        m = self.manifest
        self.assertRegex(m['commit'], r'^[0-9a-f]{40}$')
        self.assertRegex(m['patch_source']['commit'], r'^[0-9a-f]{40}$')
        self.assertIn(m['patch_source']['commit'], m['patch_source']['base_url'])
        self.assertTrue(m['patch_source']['base_url'].startswith('https://raw.githubusercontent.com/'))
        self.assertRegex(m['rust'], r'^[0-9]+\.[0-9]+\.[0-9]+$')
        self.assertRegex(m['cargo_lock_sha256'], r'^[0-9a-f]{64}$')
        self.assertEqual(len(m['patches']), 44)
        self.assertEqual(len({p['file'] for p in m['patches']}), 44)
        for p in m['patches']:
            self.assertRegex(p['sha256'], r'^[0-9a-f]{64}$')
            self.assertRegex(p['file'], r'^[a-z0-9-]+\.patch$')

    def test_recipe_changes_with_any_patch(self):
        changed = json.loads(json.dumps(self.manifest))
        changed['patches'][5]['sha256'] = '0' * 64
        self.assertNotEqual(backend.recipe(self.manifest), backend.recipe(changed))

    def test_ready_backend_is_offline_and_detects_a_changed_recipe(self):
        with tempfile.TemporaryDirectory() as temp:
            target = Path(temp) / 'xodus'
            (target / 'bin').mkdir(parents=True)
            cli = target / 'bin/xodus-cli'
            cli.write_text('#!/bin/sh\nexit 0\n')
            cli.chmod(0o755)
            self.assertFalse(backend.ready(target, self.manifest))
            (target / '.soju-xodus').write_text(backend.recipe(self.manifest) + '\n')
            self.assertFalse(backend.ready(target, self.manifest))  # no binary hash recorded
            (target / '.soju-xodus').write_text(backend.recipe(self.manifest) + '\n' + backend.digest(cli) + '\n')
            self.assertTrue(backend.ready(target, self.manifest))
            cli.write_text('#!/bin/sh\nexit 0 # swapped\n')
            self.assertFalse(backend.ready(target, self.manifest))
            (target / '.soju-xodus').write_text(backend.recipe(self.manifest) + '\n' + backend.digest(cli) + '\n')
            with patch.object(backend, 'prepare_source') as prepare:
                backend.fetch(Path(temp), self.manifest)
            prepare.assert_not_called()
            changed = json.loads(json.dumps(self.manifest))
            changed['commit'] = '1' * 40
            self.assertFalse(backend.ready(target, changed))

    def test_patch_checksum_mismatch_aborts_before_build(self):
        def run(args, **kwargs):
            if args[:2] == ['git', '-c'] and 'clone' in args:
                src = Path(args[-1])
                src.mkdir()
                (src / 'Cargo.lock').write_text('lock')
            if args[0] == 'git' and 'rev-parse' in args:
                return subprocess.CompletedProcess(args, 0, self.manifest['commit'] + '\n', '')
            if args[0] == '/usr/bin/curl':
                Path(args[args.index('--output') + 1]).write_text('tampered')
            return subprocess.CompletedProcess(args, 0, '', '')
        manifest = json.loads(json.dumps(self.manifest))
        manifest['cargo_lock_sha256'] = backend.hashlib.sha256(b'lock').hexdigest()
        with tempfile.TemporaryDirectory() as temp, patch.object(backend, 'run', side_effect=run), \
                patch.object(backend, 'build') as build:
            with self.assertRaisesRegex(ValueError, 'checksum mismatch'):
                backend.fetch(Path(temp), manifest)
            build.assert_not_called()
            self.assertFalse((Path(temp) / 'xodus').exists())

    def test_wrong_commit_or_lock_aborts(self):
        def run(args, **kwargs):
            if args[:2] == ['git', '-c'] and 'clone' in args:
                Path(args[-1]).mkdir()
                (Path(args[-1]) / 'Cargo.lock').write_text('other')
            if 'rev-parse' in args:
                return subprocess.CompletedProcess(args, 0, self.manifest['commit'] + '\n', '')
            return subprocess.CompletedProcess(args, 0, '', '')
        with tempfile.TemporaryDirectory() as temp, patch.object(backend, 'run', side_effect=run):
            with self.assertRaisesRegex(ValueError, 'Cargo.lock'):
                backend.prepare_source(Path(temp), self.manifest)


class XboxInstallerPlanTests(unittest.TestCase):
    def test_xbox_needs_the_backend_and_the_cx_engine(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp) / 'untouched'
            result = subprocess.run(['/bin/bash', str(ROOT / 'install.sh'), '--plan'],
                                    env={**os.environ, 'SOJU_PLATFORMS': 'xbox', 'SOJU_BASE': str(base)},
                                    capture_output=True, text=True)
            self.assertFalse(base.exists())
        self.assertEqual(result.returncode, 0, result.stderr)
        # Games run on the CX engine with D3DMetal, so Xbox needs those too.
        for line in ('cx_engine=1', 'apple_gptk=1', 'steam_runtime=0', 'xodus_backend=1'):
            self.assertIn(line, result.stdout)

    def test_cli_lists_xbox_commands(self):
        result = subprocess.run(['/bin/bash', str(ROOT / 'scripts/soju'), '--help'], capture_output=True, text=True)
        for command in ('xbox-install', 'xbox-login', 'xbox-sync', 'xbox-download', 'xbox-logout', 'xbox-location'):
            self.assertIn(command, result.stdout)
        bad = subprocess.run(['/bin/bash', str(ROOT / 'scripts/soju'), 'xbox-sync', 'XX'], capture_output=True, text=True)
        self.assertEqual(bad.returncode, 64)


class XboxLaunchTests(unittest.TestCase):
    def test_executable_comes_from_the_package_manifest(self):
        with tempfile.TemporaryDirectory() as temp:
            folder = Path(temp)
            (folder / 'microsoftgame.config').write_text(
                '<?xml version="1.0"?><Game configVersion="1"><ExecutableList>'
                '<Executable Name="Binaries\\Win64\\Game.exe" Id="Game"/></ExecutableList></Game>')
            self.assertEqual(runner.executable(folder), 'Binaries\\Win64\\Game.exe')
            (folder / 'microsoftgame.config').write_text('<Game><Executable Name="..\\..\\evil.exe"/></Game>')
            with self.assertRaises(runner.LaunchError):
                runner.executable(folder)
            (folder / 'microsoftgame.config').unlink()
            (folder / 'AppxManifest.xml').write_text(
                '<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10">'
                '<Applications><Application Id="App" Executable="Game.exe"/></Applications></Package>')
            self.assertEqual(runner.executable(folder), 'Game.exe')

    def test_bridge_requires_the_key_and_relays_to_the_service_socket(self):
        import socket, threading
        directory = tempfile.mkdtemp(prefix='sx', dir='/tmp')
        self.addCleanup(lambda: __import__('shutil').rmtree(directory, ignore_errors=True))
        path = directory + '/s.sock'
        server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        server.bind(path)
        server.listen(4)
        def echo():
            while True:
                try:
                    conn, _ = server.accept()
                except OSError:
                    return
                data = conn.recv(100)
                conn.sendall(b'echo:' + data)
                conn.close()
        threading.Thread(target=echo, daemon=True).start()
        bridge = runner.Bridge(path)
        self.addCleanup(bridge.close)
        self.addCleanup(server.close)
        with socket.create_connection(('127.0.0.1', bridge.port), timeout=5) as client:
            client.sendall(bridge.key + b'hello')
            self.assertEqual(client.recv(100), b'echo:hello')
        with socket.create_connection(('127.0.0.1', bridge.port), timeout=5) as client:
            client.sendall(b'0' * len(bridge.key) + b'hello')
            try:
                answer = client.recv(100)
            except ConnectionResetError:
                answer = b''
            self.assertEqual(answer, b'')  # closed, nothing relayed

    def test_runtime_dlls_lose_the_builtin_marker(self):
        with tempfile.TemporaryDirectory() as temp, patch.object(prefix, 'RUNTIME', Path(temp) / 'rt'), \
                patch.object(prefix, 'PREFIX', Path(temp) / 'pfx'):
            (Path(temp) / 'rt').mkdir()
            dll = bytearray(200)
            dll[64:80] = prefix.BUILTIN
            (Path(temp) / 'rt/xgameruntime.dll').write_bytes(bytes(dll))
            prefix.install_dlls(['xgameruntime.dll'])
            copied = (Path(temp) / 'pfx/drive_c/windows/system32/xgameruntime.dll').read_bytes()
            self.assertEqual(copied[64:80], bytes(16))
            self.assertEqual(len(copied), 200)

    def test_prefix_settings_cover_every_runtime_dll(self):
        names = json.loads((ROOT / 'resources/xbox-runtime.json').read_text())['dlls']
        values = {args[4] for args in prefix.overrides(names) if args[2].endswith('DllOverrides')}
        self.assertEqual(values, {n[:-4] for n in names} | {'concrt140'})
        self.assertTrue(any('GamingServices' in args[2] for args in prefix.overrides(names)))

    def test_runtime_manifest_pins_every_download(self):
        m = json.loads((ROOT / 'resources/xbox-runtime.json').read_text())
        for source in (m['macxus'], m['nexus']):
            self.assertTrue(source['url'].startswith('https://github.com/'))
            self.assertRegex(source['sha256'], r'^[0-9a-f]{64}$')
            self.assertGreater(source['size'], 0)
        self.assertEqual(len(m['dlls']), 6)
        for value in m['dlls'].values():
            self.assertRegex(value, r'^[0-9a-f]{64}$')
        # Soju's own xgameruntime build: one file from Soju's release, rebuildable from pinned inputs.
        soju = m['soju']
        self.assertEqual(soju['dlls'], ['xgameruntime.dll'])
        self.assertTrue(soju['url'].startswith('https://github.com/BCD1210/soju/releases/download/'))
        self.assertEqual(soju['sha256'], m['dlls']['xgameruntime.dll'])
        build = soju['build']
        self.assertRegex(build['source']['sha256'], r'^[0-9a-f]{64}$')
        self.assertRegex(build['macxus_commit'], r'^[0-9a-f]{40}$')
        ours = [p for p in build['patches'] if 'soju' in p]
        self.assertEqual(len(ours), 1)
        import hashlib
        self.assertEqual(hashlib.sha256((ROOT / ours[0]['soju']).read_bytes()).hexdigest(), ours[0]['sha256'])

    def test_compat_sets_heap_free_check_only_for_listed_games(self):
        calls = []
        def run(args, **kwargs):
            calls.append(args)
            return subprocess.CompletedProcess(args, 0, '', '')
        with patch.object(runner.subprocess, 'run', side_effect=run):
            runner.apply_compat('9MTVJ3HHTQGS', 'RetroClassics.exe')
            runner.apply_compat(SILKSONG, 'Game.exe')
            runner.apply_compat('9MTVJ3HHTQGS', 'bad"name.exe')
        self.assertEqual(len(calls), 1)
        args = calls[0]
        self.assertTrue(args[1].endswith('scripts/xbox-wine.sh'))
        self.assertEqual(args[2:4], ['reg', 'add'])
        self.assertTrue(args[4].endswith('Image File Execution Options\\RetroClassics.exe'))
        self.assertEqual(args[5:], ['/v', 'GlobalFlag', '/t', 'REG_DWORD', '/d', '0x20', '/f'])

    def test_compat_list_is_well_formed(self):
        data = json.loads((ROOT / 'resources/xbox-compat.json').read_text())
        self.assertEqual(data['schema'], 1)
        for pid, game in data['games'].items():
            self.assertRegex(pid, r'^[0-9A-Z]{12}$')
            self.assertTrue(game['why'])

    def test_store_patch_fills_every_string_the_runtime_leaves_null(self):
        patch_text = (ROOT / 'patches/xgameruntime-store-empty-strings.patch').read_text()
        for field in ('p->linkUri', 'p->price.currencyCode', 's->price.currencyCode',
                      's->collectionData.campaignId', 's->collectionData.developerOfferId'):
            self.assertIn(field, patch_text)
        self.assertIn('+        entitled_fill_strings(&page->snapshot->products[i]);', patch_text)

    def test_update_reuses_matching_dlls_and_fetches_only_soju_fix(self):
        import hashlib
        m = json.loads((ROOT / 'resources/xbox-runtime.json').read_text())
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            target = base / 'xbox-runtime'
            target.mkdir()
            fake = {}
            for name in m['dlls']:
                fake[name] = ('old-' + name).encode()
                (target / name).write_bytes(fake[name])
            pinned = dict(m, dlls={n: hashlib.sha256(fake[n]).hexdigest() for n in m['dlls']})
            new = b'fixed xgameruntime'
            pinned['dlls']['xgameruntime.dll'] = hashlib.sha256(new).hexdigest()
            pinned['soju'] = dict(m['soju'], sha256=pinned['dlls']['xgameruntime.dll'], size=len(new))
            fetched = []
            def download(url, dest, size, sha256, label):
                fetched.append(url)
                dest.write_bytes(new)
            with patch.object(runtime, 'download', side_effect=download):
                runtime.fetch(base, pinned)
            self.assertEqual(fetched, [m['soju']['url']])
            self.assertEqual((target / 'xgameruntime.dll').read_bytes(), new)
            self.assertTrue(runtime.ready(target, pinned))

    def test_asar_reader_returns_only_the_requested_folder(self):
        import struct
        files = {'a.dll': b'AAAA', 'b.txt': b'BB'}
        offsets, body = {}, b''
        for name, data in files.items():
            offsets[name] = dict(size=len(data), offset=str(len(body)))
            body += data
        header = json.dumps({'files': {'assets': {'files': {'dlls': {'files': offsets}}},
                                       'other': {'files': {}}}}).encode()
        pickle = struct.pack('<II', len(header) + 4, len(header)) + header
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / 'app.asar'
            path.write_bytes(struct.pack('<II', 4, len(pickle)) + pickle + body)
            self.assertEqual(runtime.asar_files(path, 'assets/dlls'), files)


class XboxReviewRegressionTests(unittest.TestCase):
    GAME = dict(contentId='807c7d6a-409f-48be-8190-30b09baf7cd4', kind='Game', packageFormat='MSIXVC',
                platform='Windows.Desktop', supported=True, title='Game')
    ADDON = dict(GAME, contentId='11111111-1111-4111-8111-111111111111', kind='GameAddOn', title='Soundtrack')

    def test_edition_downloads_the_game_not_its_add_on(self):
        plan = dict(schema=1, status='choice', choices=[self.GAME, self.ADDON])
        self.assertEqual(download.choose(plan, SILKSONG)['kind'], 'Game')

    def test_add_on_is_never_downloaded_as_the_game(self):
        base = dict(self.GAME, packageFormat='MSIXVC2', supported=False)
        plan = dict(schema=1, status='choice', choices=[base, self.ADDON])
        with self.assertRaisesRegex(download.DownloadError, 'newer package format'):
            download.choose(plan, SILKSONG)

    def test_seven_digit_fractions_parse_on_every_python(self):
        self.assertEqual(svc.entitlement_end('9999-12-31T23:59:59.9999999Z').year, 9999)
        self.assertEqual(svc.entitlement_end('2026-11-30T00:00:00Z').month, 11)

    def test_only_subscriptions_mark_game_pass(self):
        self.assertTrue(svc.SUBSCRIPTION.fullmatch('CFQ7TTC0KGQ8'))
        self.assertIsNone(svc.SUBSCRIPTION.fullmatch('9NBLGGH4R315'))

    def test_folder_lock_reports_a_running_download(self):
        with tempfile.TemporaryDirectory() as temp:
            cli = Path(temp) / 'xodus-cli'
            cli.write_text('#!/bin/sh\necho "another download is using this folder" >&2\nexit 1\n')
            cli.chmod(0o755)
            with patch.object(download, 'CLI', str(cli)):
                with self.assertRaisesRegex(download.DownloadError, 'already downloading'):
                    download.stream(SILKSONG, self.GAME, 'US', Path(temp))



class XboxStopAndSignOutTests(unittest.TestCase):
    """Sign-out without a backend, stopping downloads and setup, no e-mail in logs."""

    def xbox_sh(self, base, *args, cli=None):
        env = {k: v for k, v in os.environ.items() if k != 'SOJU_XODUS_BIN'}
        env['SOJU_BASE'] = str(base)
        if cli:
            env['SOJU_XODUS_BIN'] = str(cli)
        return subprocess.run(['/bin/bash', str(ROOT / 'scripts/xbox.sh'), *args], env=env,
                              capture_output=True, text=True, timeout=30)

    def test_sign_out_works_without_a_backend(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            (base / 'account-libraries').mkdir()
            (base / 'account-libraries/xbox.json').write_text('{}')
            (base / 'library-artwork').mkdir()
            (base / 'library-artwork/xbox-9N116V0599HB.jpg').write_bytes(b'jpg')
            (base / 'library-artwork/steam-1.jpg').write_bytes(b'jpg')
            result = self.xbox_sh(base, 'logout')
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertFalse((base / 'account-libraries/xbox.json').exists())
            self.assertFalse((base / 'library-artwork/xbox-9N116V0599HB.jpg').exists())
            self.assertTrue((base / 'library-artwork/steam-1.jpg').exists())

    def test_kill_stops_a_recorded_download_only(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp); run = base / 'run'; run.mkdir()
            fake = base / 'xbox-download.py'
            fake.write_text('import signal, sys, time\n'
                            'signal.signal(signal.SIGTERM, lambda *a: sys.exit(130))\n'
                            'print("ready", flush=True)\n'
                            'time.sleep(30)\n')
            other = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(30)'])
            child = subprocess.Popen([sys.executable, str(fake)], stdout=subprocess.PIPE, text=True)
            try:
                # The handler is installed before "ready", so TERM cannot hit the default action.
                self.assertEqual(child.stdout.readline().strip(), 'ready')
                (run / ('xbox-dl-' + str(child.pid) + '.pid')).write_text(str(child.pid))
                (run / ('xbox-dl-' + str(other.pid) + '.pid')).write_text(str(other.pid))
                result = self.xbox_sh(base, 'kill')
                self.assertIn('Download stopped', result.stdout)
                self.assertEqual(child.wait(timeout=10), 130)
                self.assertIsNone(other.poll(), 'a pid that is not a Soju download must be left alone')
            finally:
                for p in (child, other):
                    if p.poll() is None:
                        p.kill()
                    p.wait()

    def test_kill_reports_when_nothing_runs(self):
        with tempfile.TemporaryDirectory() as temp:
            result = self.xbox_sh(Path(temp), 'kill')
            self.assertEqual(result.returncode, 0)
            self.assertIn('No Xbox download or game is running', result.stdout)

    def test_download_records_and_removes_its_pid(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            with patch.object(download, 'BASE', base), patch.object(sys, 'argv', ['x', 'bad id']):
                self.assertEqual(download.main(), 1)
            self.assertTrue((base / 'run').is_dir())
            self.assertEqual(list((base / 'run').iterdir()), [])
        self.assertIn("'xbox-dl-'", (ROOT / 'scripts/xbox-download.py').read_text())

    def test_logs_never_show_an_e_mail(self):
        for clean in (download.clean, runner.clean):
            line = clean('Signed in as someone@example.com via https://login.example/?t=secret')
            self.assertNotIn('someone@example.com', line)
            self.assertNotIn('secret', line)
            self.assertIn('<account>', line)
        script = (ROOT / 'scripts/xbox.sh').read_text()
        self.assertIn('<account>', script)

    def test_failed_keychain_sign_out_still_removes_the_import(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            (base / 'account-libraries').mkdir()
            (base / 'account-libraries/xbox.json').write_text('{}')
            cli = base / 'xodus-cli'
            cli.write_text('#!/bin/sh\n[ "$1" = --version ] && exit 0\n[ "$1" = logout ] && exit 1\nexit 9\n')
            cli.chmod(0o755)
            result = self.xbox_sh(base, 'logout', cli=cli)
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertIn('Keychain', result.stderr)
            self.assertFalse((base / 'account-libraries/xbox.json').exists())

    def test_closed_sign_in_window_reports_not_completed(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            cli = base / 'xodus-cli'
            cli.write_text('#!/bin/sh\ncase "$1" in login) exit 1 ;; account) echo \'{"stored": false}\' ;; esac\n')
            cli.chmod(0o755)
            result = self.xbox_sh(base, 'login', cli=cli)
            self.assertEqual(result.returncode, 1)
            self.assertIn('Sign-in was not completed', result.stderr)

    def test_stop_during_first_launch_setup_ends_the_setup(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / 'repo'
            (root / 'scripts').mkdir(parents=True)
            (root / 'scripts/xbox-run.py').write_text((ROOT / 'scripts/xbox-run.py').read_text())
            (root / 'scripts/xbox_locations.py').write_text((ROOT / 'scripts/xbox_locations.py').read_text())
            # Stand-in for xbox-prefix.py: a long download running in a child process.
            (root / 'scripts/xbox-prefix.py').write_text(
                'import subprocess, sys\n'
                'child = subprocess.Popen(["/bin/sleep", "60"])\n'
                'open(sys.argv[0] + ".pid", "w").write(str(child.pid))\n'
                'print("ready", flush=True)\n'
                'child.wait()\n')
            base = Path(temp) / 'base'
            game = base / 'xbox-bottle/drive_c/XboxGames' / SILKSONG
            game.mkdir(parents=True)
            (game / '.soju-xbox-install.json').write_text('{"title": "Test"}')
            (game / '.xodus-streaming.msixvc').write_bytes(b'')
            (game / 'MicrosoftGame.config').write_text(
                '<Game><ExecutableList><Executable Name="game.exe"/></ExecutableList></Game>')
            env = dict(os.environ, SOJU_BASE=str(base), SOJU_XODUS_BIN='/usr/bin/false')
            run = subprocess.Popen([sys.executable, str(root / 'scripts/xbox-run.py'), SILKSONG], env=env,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            try:
                self.assertEqual(run.stdout.readline().strip(), 'ready', run.stderr.read() if run.poll() is not None else '')
                helper = int(Path(str(root / 'scripts/xbox-prefix.py') + '.pid').read_text())
                run.terminate()
                self.assertEqual(run.wait(timeout=20), 130)
                for _ in range(50):
                    try:
                        os.kill(helper, 0)
                    except ProcessLookupError:
                        break
                    time.sleep(0.1)
                else:
                    os.kill(helper, 9)
                    self.fail('first-launch setup kept running after Stop')
            finally:
                if run.poll() is None:
                    run.kill()
                    run.wait()
                run.stdout.close()
                run.stderr.close()

    def test_stop_during_setup_reaches_the_prefix_process_group(self):
        source = (ROOT / 'scripts/xbox-run.py').read_text()
        self.assertIn('start_new_session=True', source)
        self.assertIn('os.killpg(prep.pid, signal.SIGTERM)', source)
        self.assertNotIn("subprocess.run([sys.executable, str(ROOT / 'scripts/xbox-prefix.py')]", source)

    def test_engine_hint_matches_the_installer(self):
        for name in ('xbox-prefix.py', 'game-library.py', 'xbox.sh'):
            self.assertNotIn('Install Battle.net, Epic or GOG', (ROOT / 'scripts' / name).read_text(), name)
            self.assertNotIn('install Battle.net, Epic or GOG', (ROOT / 'scripts' / name).read_text(), name)


class XboxFirstSignInTests(unittest.TestCase):
    """What a brand-new Microsoft account runs into on its first Xbox sign-in."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)

    def cli(self, body):
        path = self.base / 'xodus/bin/xodus-cli'
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text('#!/bin/sh\n' + body)
        path.chmod(0o755)
        return path

    ROW = dict(status='Active', isTrial=False, endDate=FUTURE, satisfiedByProductId=None, special=False)
    META = {SILKSONG: dict(title='Silksong', type='Game', windows=True, format='MSIXVC', artwork=None, price='')}

    def test_an_empty_first_answer_is_asked_again_once(self):
        answers = [{'schema': 1, 'entitlements': []},
                   {'schema': 1, 'entitlements': [dict(self.ROW, productId=SILKSONG)]}]
        calls = []
        def run(args, **kwargs):
            if args[1] == 'profile':
                return subprocess.CompletedProcess(args, 0, json.dumps({'gamertag': 'Tester'}), '')
            calls.append(args)
            return subprocess.CompletedProcess(args, 0, json.dumps(answers[len(calls) - 1]), '')
        self.cli('exit 0\n')
        with patch.object(svc.subprocess, 'run', side_effect=run), patch.object(svc.time, 'sleep'), \
                patch.object(svc, 'xbox_catalog', return_value=self.META):
            games, label = svc.xbox_owned(self.base, {'market': 'US'})
        self.assertEqual(len(calls), 2)
        self.assertEqual([g['id'] for g in games], ['xbox:' + SILKSONG])

    def test_an_empty_library_is_asked_only_twice(self):
        calls = []
        def run(args, **kwargs):
            if args[1] == 'profile':
                return subprocess.CompletedProcess(args, 0, json.dumps({'gamertag': 'Tester'}), '')
            calls.append(args)
            return subprocess.CompletedProcess(args, 0, json.dumps({'schema': 1, 'entitlements': []}), '')
        self.cli('exit 0\n')
        with patch.object(svc.subprocess, 'run', side_effect=run), patch.object(svc.time, 'sleep'):
            games, _ = svc.xbox_owned(self.base, {'market': 'US'})
        self.assertEqual((len(calls), games), (2, []))

    def test_missing_xbox_profile_is_named_and_the_import_still_saves(self):
        def run(args, **kwargs):
            if args[1] == 'profile':
                return subprocess.CompletedProcess(args, 1, '', 'Xbox profile is temporarily unavailable (xsts); stored account is unchanged.\n')
            return subprocess.CompletedProcess(args, 0, json.dumps({'schema': 1, 'entitlements': [dict(self.ROW, productId=SILKSONG)]}), '')
        self.cli('exit 0\n')
        notices = []
        with patch.object(svc.subprocess, 'run', side_effect=run), patch.object(svc, 'xbox_catalog', return_value=self.META):
            games, label = svc.xbox_owned(self.base, {'market': 'US'}, notices=notices)
        self.assertEqual(len(games), 1)
        self.assertEqual(label, 'Microsoft account')
        self.assertEqual(notices, [svc.NO_XBOX_PROFILE])
        self.assertIn('xbox.com', notices[0])

    def test_download_names_the_missing_xbox_profile(self):
        cli = self.cli('case "$1" in\n'
                       '  streaming) echo "Xbox authentication request failed. Check your connection and sign-in."; exit 1 ;;\n'
                       '  profile) echo "Xbox profile is temporarily unavailable (xsts); stored account is unchanged." >&2; exit 1 ;;\n'
                       'esac\n')
        game = dict(contentId='807c7d6a-409f-48be-8190-30b09baf7cd4')
        with patch.object(download, 'CLI', str(cli)):
            with self.assertRaisesRegex(download.DownloadError, 'no Xbox profile'):
                download.stream(SILKSONG, game, 'US', self.base)

    def test_download_auth_failure_with_a_profile_is_not_blamed_on_it(self):
        cli = self.cli('case "$1" in\n'
                       '  streaming) echo "Xbox authentication request failed. Check your connection and sign-in."; exit 1 ;;\n'
                       '  profile) echo \'{"gamertag": "Tester"}\'; exit 0 ;;\n'
                       'esac\n')
        game = dict(contentId='807c7d6a-409f-48be-8190-30b09baf7cd4')
        with patch.object(download, 'CLI', str(cli)):
            self.assertFalse(download.stream(SILKSONG, game, 'US', self.base))

    def test_sign_in_says_when_the_account_has_no_xbox_profile(self):
        cli = self.cli('case "$1" in\n'
                       '  login) exit 0 ;;\n'
                       '  account) echo \'{"stored": true}\' ;;\n'
                       '  profile) echo "Xbox profile is temporarily unavailable (xsts); stored account is unchanged." >&2; exit 1 ;;\n'
                       'esac\n')
        env = dict(os.environ, SOJU_BASE=str(self.base), SOJU_XODUS_BIN=str(cli))
        result = subprocess.run(['/bin/bash', str(ROOT / 'scripts/xbox.sh'), 'login'], env=env,
                                capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('Signed in.', result.stdout)
        self.assertIn('no Xbox profile', result.stdout)
        self.assertIn('Please wait', result.stdout)


if __name__ == '__main__':
    unittest.main()
