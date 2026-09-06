import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("steam_wine", ROOT / "scripts/fetch-steam-wine.py")
wine = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wine)

class SteamWineTests(unittest.TestCase):
    def archive(self, directory, extra=()):
        archive = directory / "wine.tar.xz"
        names = ["bin/wine", "bin/wineserver", "lib/wine/x86_64-unix/ntdll.so",
                 "lib/wine/x86_64-windows/ntdll.dll", "share/wine/wine.inf"]
        with tarfile.open(archive, "w:xz") as tar:
            for name in names:
                info = tarfile.TarInfo("Wine.app/wine/" + name)
                info.mode = 0o755
                info.size = 4
                tar.addfile(info, io.BytesIO(b"wine"))
            for name, link in extra:
                info = tarfile.TarInfo("Wine.app/wine/" + name)
                if link is None:
                    info.size = 4
                    tar.addfile(info, io.BytesIO(b"wine"))
                else:
                    info.type = tarfile.SYMTYPE
                    info.linkname = link
                    tar.addfile(info)
        manifest = {"version": "test", "wine_version": "wine-11.0", "archive_root": "Wine.app/wine",
                    "sha256": wine.digest(archive), "url": "https://example.invalid/wine.tar.xz"}
        return archive, manifest

    def test_validated_private_runtime_installs_without_homebrew(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            archive, manifest = self.archive(base, [("bin/wine64", "wine")])
            with patch.object(wine, "wine_version", return_value="wine-11.0"):
                target = wine.fetch(base, manifest, archive)
                self.assertTrue(wine.ready(target, manifest))
                self.assertTrue((target / "bin/wine64").is_file())
                with patch.object(wine.subprocess, "run", side_effect=AssertionError("no download")):
                    self.assertEqual(wine.fetch(base, manifest), target)

    def test_checksum_failure_preserves_existing_runtime(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            archive, manifest = self.archive(base)
            target = base / "steam-wine"
            target.mkdir()
            (target / "keep").write_text("previous")
            manifest["sha256"] = "0" * 64
            with self.assertRaisesRegex(ValueError, "checksum"):
                wine.fetch(base, manifest, archive)
            self.assertEqual((target / "keep").read_text(), "previous")

    def test_rejects_traversal_external_links_and_duplicates(self):
        for extra in [[("../escape", None)], [("bin/out", "/tmp/outside")],
                      [("bin/out", "../../outside")], [("bin/wine", None)],
                      [("bin/dir", "wine"), ("bin/dir/escape", None)]]:
            with self.subTest(extra=extra), tempfile.TemporaryDirectory() as tmp:
                base = Path(tmp)
                archive, manifest = self.archive(base, extra)
                dest = base / "stage"
                dest.mkdir()
                with self.assertRaises((ValueError, OSError)):
                    wine.unpack(archive, dest, manifest)

    def test_failed_runtime_validation_preserves_previous(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            archive, manifest = self.archive(base)
            target = base / "steam-wine"
            target.mkdir()
            (target / "keep").write_text("previous")
            with patch.object(wine, "wine_version", return_value="wine-12.0"):
                with self.assertRaisesRegex(ValueError, "Unexpected Wine version"):
                    wine.fetch(base, manifest, archive)
            self.assertEqual((target / "keep").read_text(), "previous")

    def test_private_runtime_selection_and_explicit_override(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            def select(override=None):
                env = {**os.environ, "SOJU_BASE": str(base)}
                env.pop("SOJU_STEAM_WINE", None)
                if override: env["SOJU_STEAM_WINE"] = override
                return subprocess.check_output(["/bin/bash", "-c",
                    'source "$1"; printf "%s" "$STEAM_WINE_ROOT"', "test",
                    str(ROOT / "scripts/steam-runtime.sh")], env=env, text=True)
            self.assertEqual(select(), "/Applications/Wine Stable.app/Contents/Resources/wine")
            for name, marker in [("steam-wine", ".soju-wine"), ("steam-runtime", ".soju-runtime")]:
                target = base / name
                (target / "bin").mkdir(parents=True)
                (target / marker).touch()
                (target / "bin/wine").write_text("#!/bin/sh\nexit 0\n")
                (target / "bin/wine").chmod(0o755)
                self.assertEqual(select(), str(target))
            (base / "steam-runtime/bin/wine").unlink()
            self.assertEqual(select(), str(base / "steam-wine"))
            self.assertEqual(select("/custom/wine"), "/custom/wine")

class SteamFirstLaunchTests(unittest.TestCase):
    def test_first_launch_downloads_client_and_existing_client_keeps_wrapper(self):
        for installed in (False, True):
            with self.subTest(installed=installed), tempfile.TemporaryDirectory() as tmp:
                base = Path(tmp)
                prefix = base / "steam-bottle"
                steam = prefix / "drive_c/Program Files (x86)/Steam"
                steam.mkdir(parents=True)
                (steam / "steam.exe").touch()
                if installed:
                    (steam / "steamui.dll").touch()
                runtime = base / "wine"
                (runtime / "bin").mkdir(parents=True)
                log = base / "args"
                binary = runtime / "bin/wine"
                binary.write_text('#!/bin/bash\nprintf "%s\\n" "$@" >> "$SOJU_TEST_ARGS"\n')
                binary.chmod(0o755)
                stubs = base / "stubs"
                stubs.mkdir()
                for name in ("python3", "pgrep"):
                    script = stubs / name
                    script.write_text("#!/bin/bash\nexit 0\n")
                    script.chmod(0o755)
                env = {**os.environ, "SOJU_BASE": str(base), "WINEPREFIX": str(prefix),
                       "SOJU_STEAM_WINE": str(runtime), "SOJU_TEST_ARGS": str(log),
                       "WINE_VIRTUAL_DESKTOP": "", "SOJU_KEYLOG": "0",
                       "PATH": str(stubs) + ":/usr/bin:/bin"}
                result = subprocess.run(["/bin/bash", str(ROOT / "scripts/play.sh"), "steam"],
                                        env=env, capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                args = log.read_text().splitlines()
                self.assertEqual("-noverifyfiles" in args, installed)
                self.assertIn("-cef-single-process", args)

spec_bootstrap = importlib.util.spec_from_file_location("steam_bootstrap", ROOT / "scripts/bootstrap-steam.py")
bootstrap = importlib.util.module_from_spec(spec_bootstrap)
spec_bootstrap.loader.exec_module(bootstrap)

class SteamBootstrapTests(unittest.TestCase):
    def test_only_final_verification_finishes_initial_download(self):
        verified = "[time] Startup - updater built old\n[time] Verification complete\n"
        for phase in ["Downloading update", "Extracting package", "Installing update", "Startup - updater built new"]:
            self.assertFalse(bootstrap.verification_finished(verified + phase))
        self.assertTrue(bootstrap.verification_finished(verified + "Installing update\nStartup - updater built new\nVerification complete\n"))
        self.assertFalse(bootstrap.verification_finished(""))

    def test_complete_client_never_starts_or_stops_wine(self):
        with tempfile.TemporaryDirectory() as tmp:
            prefix = Path(tmp)
            steam = prefix / "drive_c/Program Files (x86)/Steam"
            cef = steam / "bin/cef/cef.win7x64"
            cef.mkdir(parents=True)
            (steam / "steamui.dll").touch()
            (cef / "steamwebhelper.exe").touch()
            with patch.object(bootstrap.subprocess, "Popen", side_effect=AssertionError("existing Steam")), \
                 patch.object(bootstrap.subprocess, "run", side_effect=AssertionError("existing Steam")):
                bootstrap.bootstrap(prefix / "wine/bin/wine", prefix)

    def test_bootstrapper_relaunch_is_not_mistaken_for_completed_download(self):
        with tempfile.TemporaryDirectory() as tmp:
            prefix = Path(tmp)
            steam = prefix / "drive_c/Program Files (x86)/Steam"
            (steam / "logs").mkdir(parents=True)
            (steam / "steam.exe").touch()
            from unittest.mock import Mock
            process = Mock()
            process.poll.return_value = 42
            process.wait.return_value = 42
            def start(*args, **kwargs):
                cef = steam / "bin/cef/cef.win7x64"
                cef.mkdir(parents=True)
                (steam / "steamui.dll").touch()
                (cef / "steamwebhelper.exe").touch()
                (steam / "logs/bootstrap_log.txt").write_text("Startup - updater built new\nVerification complete\n")
                return process
            with patch.object(bootstrap.subprocess, "Popen", side_effect=start), \
                 patch.object(bootstrap.subprocess, "run") as stop:
                bootstrap.bootstrap(prefix / "wine/bin/wine", prefix)
            self.assertEqual(len(stop.call_args_list), 2)
            for call in stop.call_args_list:
                self.assertEqual(call.kwargs["env"]["WINEPREFIX"], str(prefix))

    def test_incomplete_download_reports_failure_after_scoped_cleanup(self):
        with tempfile.TemporaryDirectory() as tmp:
            prefix = Path(tmp)
            with patch.object(bootstrap.subprocess, "Popen"), patch.object(bootstrap.subprocess, "run") as stop, \
                 patch.object(bootstrap.time, "monotonic", side_effect=[0, 1000]):
                with self.assertRaisesRegex(RuntimeError, "did not finish"):
                    bootstrap.bootstrap(prefix / "wine/bin/wine", prefix, timeout=1)
            self.assertEqual(stop.call_args.kwargs["env"]["WINEPREFIX"], str(prefix))
