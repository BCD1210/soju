"""Steam lifecycle regressions. Only temporary prefixes and fake OS tools are used."""
import importlib.util
import os
from pathlib import Path
import shlex
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("steam_session", ROOT / "scripts/steam-session.py")
session = importlib.util.module_from_spec(spec)
spec.loader.exec_module(session)


def executable(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("#!/bin/bash\n" + text)
    path.chmod(0o755)


class SteamSessionTests(unittest.TestCase):
    def test_all_managed_and_explicit_server_paths_block_migration(self):
        for path in ["/Applications/Wine Stable.app/Contents/Resources/wine/bin/wineserver",
                     "/private/tmp/soju/steam-wine/bin/wineserver",
                     "/tmp/soju/steam-runtime/bin/wineserver",
                     "/tmp/soju/steam-runtime.previous/bin/wineserver",
                     "/tmp/soju/steam-wine.previous/bin/wineserver",
                     "/tmp/soju/.steam-runtime.ABC/bin/wineserver",
                     "/custom/wine/bin/wineserver64"]:
            with self.subTest(path=path), patch.object(session, "server_paths", return_value=[Path(path)]):
                with self.assertRaisesRegex(RuntimeError, "Close Windows Steam"):
                    session.require_idle(Path("/custom/wine"))

    def test_other_platform_server_does_not_block_steam(self):
        with patch.object(session, "server_paths", return_value=[Path("/tmp/soju/cx26-engine/bin/wineserver")]):
            session.require_idle(Path("/tmp/soju/steam-runtime"))

    def test_failed_process_query_aborts_migration(self):
        with patch.object(session.subprocess, "run", side_effect=subprocess.CalledProcessError(1, "ps")):
            with self.assertRaises(subprocess.CalledProcessError):
                session.require_idle(Path("/tmp/wine"))

    def test_stop_scopes_and_cleans_environment_and_waits(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t); prefix = root / "prefix"; prefix.mkdir()
            runtime = root / "runtime"; executable(runtime / "bin/wineserver", "exit 0\n")
            with patch.dict(os.environ, {"WINEPREFIX": "/unrelated", "WINESERVER": "/wrong",
                                        "DYLD_FALLBACK_LIBRARY_PATH": "/wrong", "CX_TEST": "bad"}), \
                 patch.object(session.subprocess, "run") as run:
                session.stop(prefix, runtime)
            self.assertEqual([c.args[0][-1] for c in run.call_args_list], ["-k", "-w"])
            for call in run.call_args_list:
                env = call.kwargs["env"]
                self.assertEqual(env["WINEPREFIX"], str(prefix.resolve()))
                self.assertNotIn("WINESERVER", env)
                self.assertNotIn("DYLD_FALLBACK_LIBRARY_PATH", env)
                self.assertNotIn("CX_TEST", env)
                self.assertTrue(call.kwargs["check"])
                self.assertEqual(call.kwargs["timeout"], 30)

    def test_timeout_propagates_instead_of_allowing_delete(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t); prefix = root / "prefix"; prefix.mkdir()
            executable(root / "runtime/bin/wineserver", "exit 0\n")
            with patch.object(session.subprocess, "run", side_effect=subprocess.TimeoutExpired("wineserver", 30)):
                with self.assertRaises(subprocess.TimeoutExpired):
                    session.stop(prefix, root / "runtime")


class SteamInstallRecoveryTests(unittest.TestCase):
    def test_existing_client_still_runs_runtime_preparation_before_renderer(self):
        stage = (ROOT / "install.sh").read_text().split("# ---------- 3. Launchers ----------")[1].split("# ---------- 4. App bundles")[0]
        with tempfile.TemporaryDirectory() as t:
            d = Path(t)
            client = d / "steam-bottle/drive_c/Program Files (x86)/Steam/steam.exe"
            client.parent.mkdir(parents=True); client.touch()
            executable(d / "scripts/create-steam-bottle.sh", 'touch "$SOJU_BASE/prepared"\n')
            executable(d / "scripts/setup-steam-games.sh", 'test -f "$SOJU_BASE/prepared"\n')
            pre = 'set -euo pipefail\nBASE="$SOJU_BASE"\nSOJU_DIR="$BASE"\nPLATFORMS=steam\nsay(){ :; }\n'
            result = subprocess.run(["/bin/bash", "-c", pre + stage],
                                    env={**os.environ, "SOJU_BASE": t}, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue((d / "prepared").exists())

    def test_missing_incompatible_or_damaged_runtime_is_repaired_for_existing_client(self):
        for existing in ("missing", "wrong-version", "missing-server"):
            with self.subTest(existing=existing), tempfile.TemporaryDirectory() as t:
                d = Path(t); base = d / "base"; project = d / "project"; stubs = d / "stubs"
                (project / "scripts").mkdir(parents=True)
                for name in ("steam-runtime.sh", "create-steam-bottle.sh", "steam-session.py"):
                    (project / "scripts" / name).write_text((ROOT / "scripts" / name).read_text())
                client = base / "steam-bottle/drive_c/Program Files (x86)/Steam/steam.exe"
                client.parent.mkdir(parents=True); client.write_text("existing client")
                # Use a missing legacy path even on developer Macs with Wine installed.
                p = project / "scripts/steam-runtime.sh"
                p.write_text(p.read_text().replace("/Applications/Wine Stable.app/Contents/Resources/wine",
                                                   str(d / "missing-legacy")))
                if existing != "missing":
                    runtime = base / "steam-runtime"
                    executable(runtime / "bin/wine", "echo wine-12.0\n" if existing == "wrong-version" else "echo wine-11.0\n")
                    (runtime / ".soju-runtime").touch()
                    if existing != "missing-server":
                        executable(runtime / "bin/wineserver", "exit 0\n")
                executable(stubs / "ps", "exit 0\n")
                executable(stubs / "python3", r'''
case "$1" in
  -) cat >/dev/null;;
  */fetch-steam-wine.py)
    mkdir -p "$SOJU_BASE/steam-wine/bin"
    printf '#!/bin/bash\necho wine-11.0\n' > "$SOJU_BASE/steam-wine/bin/wine"
    printf '#!/bin/bash\nexit 0\n' > "$SOJU_BASE/steam-wine/bin/wineserver"
    chmod +x "$SOJU_BASE/steam-wine/bin/"*
    touch "$SOJU_BASE/steam-wine/.soju-wine"
    echo fetch >> "$SOJU_BASE/events";;
  */fetch-steam-support.py) :;;
  */bootstrap-steam.py) printf '%s\n' "$2" >> "$SOJU_BASE/events";;
  *) exec /usr/bin/python3 "$@";;
esac
''')
                env = {k: v for k, v in os.environ.items() if k not in ("SOJU_STEAM_WINE", "WINEPREFIX")}
                env.update(SOJU_BASE=str(base), PATH=str(stubs)+":/usr/bin:/bin")
                result = subprocess.run(["/bin/bash", str(project / "scripts/create-steam-bottle.sh")],
                                        env=env, capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual((base / "events").read_text().splitlines(),
                                 ["fetch", str(base / "steam-wine/bin/wine")])
                self.assertEqual(client.read_text(), "existing client")

    def test_invalid_explicit_override_never_downloads_or_replaces_it(self):
        with tempfile.TemporaryDirectory() as t:
            d = Path(t); executable(d / "stubs/python3", 'echo unexpected-download; exit 88\n')
            env = {**os.environ, "SOJU_STEAM_WINE": str(d / "custom"),
                   "PATH": str(d / "stubs") + ":/usr/bin:/bin"}
            result = subprocess.run(["/bin/bash", "-c",
                'source "$1"; soju_ensure_steam_runtime "$2"', "test",
                str(ROOT / "scripts/steam-runtime.sh"), str(ROOT)], env=env, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertNotIn("unexpected-download", result.stdout)
            self.assertFalse((d / "custom").exists())


class SteamUninstallTests(unittest.TestCase):
    def scenario(self, failure="", answers=None, missing=False, live=False, custom=False, ps_failure=False):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        d = Path(temp.name); base = d / "base"; project = d / "project"; stubs = d / "stubs"
        prefix = base / "steam-bottle"; prefix.mkdir(parents=True)
        (prefix / "saved-game").write_text("keep until stopped")
        (d / "other-install").mkdir(); (d / "other-install/game").write_text("untouched")
        for name, marker in [("steam-runtime", ".soju-runtime"), ("steam-wine", ".soju-wine")]:
            executable(base / name / "bin/wine", "echo wine-11.0\n")
            (base / name / marker).touch()
            if not missing:
                executable(base / name / "bin/wineserver", r'''
printf '%s %s\n' "$WINEPREFIX" "$1" >> "$SOJU_TEST/events"
test "$1" != "$SOJU_TEST_FAILURE" || exit 7
test -f "$WINEPREFIX/saved-game" || exit 8
# Model a final shutdown write while -w waits.
if [ "$1" = -w ]; then echo final > "$WINEPREFIX/shutdown-write"; fi
''')
        for name in ("steam-runtime.previous", "steam-wine.previous"):
            (base / name).mkdir()
        if custom:
            import shutil
            shutil.copytree(base / "steam-runtime", d / "custom-runtime")
        executable(stubs / "ps", ("exit 2\n" if ps_failure else
                   'echo "/tmp/live/steam-wine/bin/wineserver"\n' if live else "exit 0\n"))
        (project / "scripts").mkdir(parents=True)
        for name in ("steam-runtime.sh", "steam-session.py", "uninstall.sh"):
            (project / "scripts" / name).write_text((ROOT / "scripts" / name).read_text())
        args = ["--yes"]
        if answers is not None:
            answer_file = d / "answers"; answer_file.write_text(answers)
            p = project / "scripts/uninstall.sh"
            p.write_text(p.read_text().replace("TTY=/dev/tty",
                "exec 9<" + shlex.quote(str(answer_file)) + "\nTTY=/dev/fd/9"))
            args = []
        env = {k: v for k, v in os.environ.items() if k not in ("SOJU_STEAM_WINE", "ENGINE", "WINEPREFIX")}
        env.update(SOJU_BASE=str(base), SOJU_TEST=str(d), SOJU_TEST_FAILURE=failure,
                   PATH=str(stubs)+":/usr/bin:/bin")
        if custom: env["SOJU_STEAM_WINE"] = str(d / "custom-runtime")
        result = subprocess.run(["/bin/bash", str(project / "scripts/uninstall.sh"), *args],
                                env=env, capture_output=True, text=True, timeout=15)
        self.assertEqual((d / "other-install/game").read_text(), "untouched")
        return d, base, result

    def test_yes_waits_then_removes_prefix_and_all_managed_runtimes(self):
        d, base, result = self.scenario()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(base.exists())
        self.assertEqual([line.rsplit(" ", 1)[1] for line in (d / "events").read_text().splitlines()], ["-k", "-w"])

    def test_failed_kill_or_wait_preserves_game_and_runtime(self):
        for failure in ("-k", "-w"):
            with self.subTest(failure=failure):
                d, base, result = self.scenario(failure=failure)
                self.assertNotEqual(result.returncode, 0)
                self.assertTrue((base / "steam-bottle/saved-game").exists())
                self.assertTrue((base / "steam-runtime/bin/wine").exists())

    def test_declining_all_does_not_stop_or_remove_anything(self):
        d, base, result = self.scenario(answers="n\nn\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((base / "steam-bottle/saved-game").exists())
        self.assertTrue((base / "steam-wine").exists())
        self.assertFalse((d / "events").exists())

    def test_runtime_only_removal_stops_retained_prefix_first(self):
        d, base, result = self.scenario(answers="n\ny\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((base / "steam-bottle/shutdown-write").exists())
        self.assertFalse((base / "steam-runtime").exists())
        self.assertEqual(len((d / "events").read_text().splitlines()), 2)

    def test_missing_engine_can_be_cleaned_when_no_servers_exist(self):
        d, base, result = self.scenario(missing=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(base.exists())

    def test_missing_engine_with_live_or_unknown_server_preserves_prefix(self):
        for kwargs in ({"live": True}, {"ps_failure": True}):
            with self.subTest(kwargs=kwargs):
                d, base, result = self.scenario(missing=True, **kwargs)
                self.assertNotEqual(result.returncode, 0)
                self.assertTrue((base / "steam-bottle/saved-game").exists())

    def test_explicit_external_runtime_is_used_but_never_deleted(self):
        d, base, result = self.scenario(custom=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(base.exists())
        self.assertTrue((d / "custom-runtime/bin/wineserver").exists())
