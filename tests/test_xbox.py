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
        with self.assertRaisesRegex(ValueError, 'same engine'):
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
        for command in ('xbox-install', 'xbox-login', 'xbox-sync', 'xbox-download', 'xbox-logout'):
            self.assertIn(command, result.stdout)
        bad = subprocess.run(['/bin/bash', str(ROOT / 'scripts/soju'), 'xbox-sync', 'XX'], capture_output=True, text=True)
        self.assertEqual(bad.returncode, 64)


if __name__ == '__main__':
    unittest.main()


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
