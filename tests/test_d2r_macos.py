"""D2R preflight with fake OS, Wine and prefixes; never launch real games."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class D2RMacOSTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='soju-d2r-test-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.scripts = self.root / 'scripts'
        self.scripts.mkdir()
        for name in ('doctor.sh', 'play.sh', 'steam-runtime.sh', 'd2r-macos.sh'):
            source = ROOT / 'scripts' / name
            if source.exists(): shutil.copy(source, self.scripts / name)
        # The production doctor uses an absolute read-only process query.
        doctor = self.scripts / 'doctor.sh'
        doctor.write_text(doctor.read_text().replace('/usr/bin/pgrep', 'pgrep'))
        self.bin = self.root / 'bin'
        self.bin.mkdir()
        for name, body in {
            'sw_vers': 'printf "%s\\n" "$TEST_MACOS"; exit "${TEST_SWVERS_EXIT:-0}"',
            'uname': 'echo arm64', 'sysctl': 'echo TestCPU',
            'pgrep': '[ "$*" = "-q oahd" ]', 'xcode-select': 'exit 0',
            'df': 'printf "Filesystem Blocks Used Available\\ntest 200 50 150\\n"',
            'defaults': 'exit 1', 'curl': 'echo \'[{"tag_name":"engine-v1.5"}]\'',
            'python3': 'echo python >> "$TEST_CALLS"',
        }.items(): self.executable(self.bin / name, body)
        self.base = self.root / 'base'
        self.prefix = self.base / 'bottle'
        self.game = self.prefix / 'drive_c/Program Files (x86)/Diablo II Resurrected'
        self.game.mkdir(parents=True)
        bn = self.prefix / 'drive_c/Program Files (x86)/Battle.net'
        bn.mkdir()
        (bn / 'Battle.net.exe').touch()
        self.engine = self.base / 'cx26-engine'
        (self.engine / 'bin').mkdir(parents=True)
        self.executable(self.engine / 'bin/wine',
                        'if [ "$1" = --version ]; then echo wine-11.0; else printf "%s\\n" "$*" >> "$TEST_CALLS"; fi')
        (self.engine / '.soju-engine-release').write_text('engine-v1.5')
        ext = self.engine / 'lib/external'
        (ext / 'D3DMetal.framework').mkdir(parents=True)
        (ext / 'libd3dshared.dylib').touch()
        for arch in ('x86_64-windows', 'x86_64-unix'):
            (self.engine / 'lib/wine' / arch).mkdir(parents=True)
        for name in ('d3d11', 'd3d12', 'dxgi'):
            (self.engine / 'lib/wine/x86_64-windows' / (name + '.dll')).write_text('D3DMetalDLLsBase')
            (self.engine / 'lib/wine/x86_64-unix' / (name + '.so')).symlink_to('../../external/libd3dshared.dylib')
        self.calls = self.root / 'calls'

    def executable(self, path, body):
        path.write_text('#!/bin/bash\n' + body + '\n')
        path.chmod(0o755)

    def run_script(self, name, version, *args, swvers_exit=0):
        self.calls.unlink(missing_ok=True)
        env = dict(os.environ, SOJU_BASE=str(self.base), ENGINE=str(self.engine),
                   WINEPREFIX=str(self.prefix), PATH=str(self.bin) + ':/usr/bin:/bin',
                   TEST_MACOS=version, TEST_SWVERS_EXIT=str(swvers_exit),
                   TEST_CALLS=str(self.calls), SOJU_KEYLOG='0')
        return subprocess.run(['/bin/bash', str(self.scripts / name), *args],
                              env=env, capture_output=True, text=True, timeout=10)

    def test_old_macos_stops_direct_game_before_side_effects(self):
        for version in ('15.7', '26.0.1', '26.3.9'):
            with self.subTest(version=version):
                result = self.run_script('play.sh', version, 'd2r')
                self.assertNotEqual(result.returncode, 0)
                self.assertIn('macOS 26.4', result.stderr)
                self.assertFalse(self.calls.exists())

    def test_library_play_cannot_bypass_check(self):
        result = self.run_script('play.sh', '26.0.1', 'battlenet', '--exec=launch OSI')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('macOS 26.4', result.stderr)
        self.assertFalse(self.calls.exists())

    def test_supported_versions_launch_and_keep_arguments(self):
        for version in ('26.4', '26.4.1', '26.10', '27.0'):
            with self.subTest(version=version):
                result = self.run_script('play.sh', version, 'd2r', '-test')
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn('D2R.exe -test', self.calls.read_text())

    def test_unknown_or_failed_version_query_stops_direct_game(self):
        for version, code in (('', 0), ('unknown', 0), ('26', 0), ('26.5', 1)):
            with self.subTest(version=version, code=code):
                result = self.run_script('play.sh', version, 'd2r', swvers_exit=code)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn('could not determine', result.stderr)
                self.assertFalse(self.calls.exists())

    def test_battlenet_warns_but_other_games_remain_available(self):
        result = self.run_script('play.sh', '26.0.1', 'battlenet', '--exec=launch WoW')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('macOS 26.4', result.stderr)
        self.assertIn('--exec=launch WoW', self.calls.read_text())

    def test_other_platform_does_not_inherit_d2r_requirement(self):
        exe = self.prefix / 'drive_c/Program Files/Epic Games/Launcher/Portal/Binaries/Win64/EpicGamesLauncher.exe'
        exe.parent.mkdir(parents=True)
        exe.touch()
        self.executable(self.scripts / 'soju-reaper.sh', 'exit 0')
        result = self.run_script('play.sh', '15.7', 'epic')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn('D2R', result.stderr)

    def test_doctor_warns_in_installed_battlenet_and_all_scopes(self):
        for scope in ('installed', 'battlenet', 'all'):
            with self.subTest(scope=scope):
                result = self.run_script('doctor.sh', '26.0.1', scope)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn('warn  D2R', result.stdout)
                self.assertNotIn('All good', result.stdout)
                self.assertIn('review the warnings', result.stdout)

    def test_explicit_d2r_diagnosis_fails_on_old_os(self):
        result = self.run_script('doctor.sh', '26.0.1', 'd2r')
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn('FAIL  D2R', result.stdout)

    def test_doctor_unknown_version_is_not_green(self):
        result = self.run_script('doctor.sh', '', 'battlenet')
        self.assertIn('warn  D2R', result.stdout)
        self.assertNotIn('All good', result.stdout)

    def test_doctor_supported_os_does_not_claim_game_was_tested(self):
        result = self.run_script('doctor.sh', '26.5', 'd2r')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('macOS requirement met', result.stdout)
        self.assertIn('game compatibility is not verified', result.stdout)

    def test_steam_epic_gog_doctor_do_not_warn_about_d2r(self):
        for scope in ('steam', 'epic', 'gog'):
            with self.subTest(scope=scope):
                result = self.run_script('doctor.sh', '26.0.1', scope)
                self.assertNotIn('D2R', result.stdout)

    def test_empty_install_does_not_warn_about_d2r(self):
        shutil.rmtree(self.prefix)
        result = self.run_script('doctor.sh', '26.0.1')
        self.assertNotIn('D2R', result.stdout)
