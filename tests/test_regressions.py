"""Run on macOS: python3 -m unittest discover -s tests -v.
All process APIs are mocked; Wine and installed games are never launched/killed.
"""
import os
from pathlib import Path
import shlex
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
BASH = "/bin/bash"


def run(code, env=None, args=()):
    return subprocess.run(
        [BASH, "-c", code, "probe", *args], text=True, capture_output=True,
        timeout=15, start_new_session=True, env={**os.environ, **(env or {})})


class ReaperTests(unittest.TestCase):
    def probe(self, frames, identities=None):
        with tempfile.TemporaryDirectory(prefix="soju-test-") as t:
            # Insert mocked OS queries before the unchanged production loop.
            script = (ROOT / "scripts/soju-reaper.sh").read_text()
            boundary = script.index("declare -a STRIKES")
            identities = identities or ["original"] * len(frames)
            mocks = """
server_alive(){ return 0; }
bottle_pids(){ echo 42001; }
classify(){ ALIVE_PIDS='42001'; GAME_PIDS='42001'; HELPER_PIDS=''; }
exe_of(){ echo D2R.exe; }
kill(){ echo "SIGNAL $*"; }
"""
            mocks += "FRAMES=(" + " ".join(shlex.quote(x) for x in frames) + ")\n"
            mocks += "IDENTITY_FRAMES=(" + " ".join(shlex.quote(x) for x in identities) + ")\n"
            mocks += """
N=-1
sleep(){ N=$((N+1)); [ "$N" -lt "${#FRAMES[@]}" ] || exit 0; }
window_pids(){ [ "${FRAMES[$N]}" != error ] || return 1; echo "${FRAMES[$N]}"; }
ps(){ echo "${IDENTITY_FRAMES[$N]}"; }
"""
            p = Path(t) / "reaper.sh"
            p.write_text(script[:boundary] + mocks + script[boundary:])
            result = subprocess.run(
                [BASH, str(p), t, "/no-real-wineserver", "battlenet"],
                capture_output=True, text=True, timeout=15)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stderr, "")
            return result.stdout

    def test_first_window_can_take_many_rounds(self):
        self.assertNotIn("SIGNAL", self.probe(["42002"] * 12))

    def test_exited_window_is_still_reaped(self):
        out = self.probe(["42001", "42002", "42002", "42002"])
        self.assertEqual(out.count("SIGNAL -9 42001"), 1)

    def test_window_return_resets_strikes(self):
        self.assertNotIn("SIGNAL", self.probe(
            ["42001", "42002", "42002", "42001", "42002", "42002"]))

    def test_reused_pid_has_no_previous_window_history(self):
        self.assertNotIn("SIGNAL", self.probe(
            ["42001", "42002", "42002", "42002", "42002"],
            ["old", "new", "new", "new", "new"]))

    def test_failed_window_query_does_not_count(self):
        self.assertNotIn("SIGNAL", self.probe(
            ["42001", "error", "error", "error", "42002", "42002"]))


class SweepTests(unittest.TestCase):
    def probe(self, candidates, comm, listing, servers="", cwd="", lsof_exit=0,
              dry=False, changed=False):
        env = {
            "MOCK_CAND": candidates, "MOCK_COMM": comm, "MOCK_LISTING": listing,
            "MOCK_SERVERS": servers, "MOCK_CWD": cwd,
            "MOCK_LSOF_EXIT": str(lsof_exit), "SOJU_SWEEP_DRY": str(int(dry))}
        pre = r"""
pgrep(){
  case "$1" in
    -if|-f) printf '%s\n' "$MOCK_CAND";;
    -x) [ "$2" = wineserver ] && printf '%s\n' "$MOCK_SERVERS";;
  esac
}
ps(){
  case "$2" in comm=) printf '%s\n' "$MOCK_COMM";; lstart=) echo original;; esac
}
lsof(){
  case " $* " in
    *" -d cwd "*) printf '%s\n' "$MOCK_CWD";;
    *) printf '%s\n' "$MOCK_LISTING"; return "$MOCK_LSOF_EXIT";;
  esac
}
kill(){ echo "SIGNAL $*"; }
"""
        if changed:
            pre += 'server_pids(){ :; }\n'  # Production definition is tested below via pgrep.
            with tempfile.TemporaryDirectory(prefix="soju-test-") as t:
                counter = Path(t) / "server-count"
                env["MOCK_COUNTER"] = str(counter)
                pre += r"""
pgrep(){
  case "$1" in
    -if|-f) printf '%s\n' "$MOCK_CAND";;
    -x)
      [ "$2" = wineserver ] || return 1
      if [ -e "$MOCK_COUNTER" ]; then echo 43000; else : > "$MOCK_COUNTER"; fi;;
  esac
}
"""
                result = run(pre + "\nsource " + shlex.quote(str(ROOT / "scripts/soju-sweep.sh")), env)
        else:
            result = run(pre + "\nsource " + shlex.quote(str(ROOT / "scripts/soju-sweep.sh")), env)
        self.assertNotIn("unbound", result.stderr)
        return result

    def test_log_reader_is_not_a_service(self):
        x = self.probe("42001", "/usr/bin/tail", "p42001\nn/tmp/services.exe.log")
        self.assertNotIn("SIGNAL", x.stdout)
        self.assertEqual(x.returncode, 0)

    def test_real_orphan_is_removed(self):
        x = self.probe("42001", r"C:\windows\system32\services.exe",
                       "p42001\nn/tmp/bottle/drive_c/windows")
        self.assertIn("SIGNAL -9 42001", x.stdout)

    def test_live_service_is_preserved(self):
        x = self.probe("42001", r"C:\windows\system32\services.exe",
                       "p42001\nn/tmp/bottle/drive_c/windows\nn/private/tmp/server/tmpmap-1",
                       "43000", "p43000\nn/private/tmp/server")
        self.assertNotIn("SIGNAL", x.stdout)

    def test_missing_pid_is_unknown(self):
        x = self.probe("42001\n42002", r"C:\windows\system32\services.exe",
                       "p42001\nn/tmp/bottle/drive_c/windows\nn/private/tmp/server/tmpmap-1",
                       "43000", "p43000\nn/private/tmp/server")
        self.assertNotIn("SIGNAL", x.stdout)

    def test_partial_lsof_error_is_unknown(self):
        x = self.probe("42001", r"C:\windows\system32\services.exe",
                       "p42001\nn/tmp/bottle/drive_c/windows", lsof_exit=1)
        self.assertNotIn("SIGNAL", x.stdout)
        self.assertNotEqual(x.returncode, 0)

    def test_missing_server_cwd_is_unknown(self):
        x = self.probe("42001", r"C:\windows\system32\services.exe",
                       "p42001\nn/tmp/bottle/drive_c/windows",
                       "43000\n43001", "p43000\nn/private/tmp/server")
        self.assertNotIn("SIGNAL", x.stdout)
        self.assertNotEqual(x.returncode, 0)

    def test_no_wine_prefix_evidence_is_preserved(self):
        x = self.probe("42001", r"C:\windows\system32\services.exe", "p42001\nn/tmp/other")
        self.assertNotIn("SIGNAL", x.stdout)

    def test_dry_run_never_signals(self):
        x = self.probe("42001", r"C:\windows\system32\services.exe",
                       "p42001\nn/tmp/bottle/drive_c/windows", dry=True)
        self.assertIn("would remove 42001", x.stdout)
        self.assertNotIn("SIGNAL", x.stdout)

    def test_new_server_invalidates_snapshot(self):
        x = self.probe("42001", r"C:\windows\system32\services.exe",
                       "p42001\nn/tmp/bottle/drive_c/windows", changed=True)
        self.assertNotIn("SIGNAL", x.stdout)
        self.assertNotEqual(x.returncode, 0)


class UninstallTests(unittest.TestCase):
    def scenario(self, answers=None, yes=False, empty=False):
        with tempfile.TemporaryDirectory(prefix="soju-test-") as t:
            d = Path(t)
            base = d / "base"
            engine = base / "cx26-engine"
            base.mkdir()
            marker = d / "stopped"
            if not empty:
                (engine / "bin").mkdir(parents=True)
                server = engine / "bin/wineserver"
                server.write_text("#!/bin/bash\necho \"$*\" >> " + shlex.quote(str(marker)) + "\n")
                server.chmod(0o755)
                (base / "bottle").mkdir()
                (base / "test.part").write_text("keep")
            # Only replace the terminal path so each production ask() reads
            # an explicit answer without acquiring the user's terminal.
            src = (ROOT / "scripts/uninstall.sh").read_text()
            if answers is not None:
                answer_file = d / "answers"
                answer_file.write_text(answers)
                # Keep one descriptor open so consecutive read calls advance.
                src = src.replace("TTY=/dev/tty",
                                  "exec 9<" + shlex.quote(str(answer_file)) + "\nTTY=/dev/fd/9")
            script = d / "uninstall.sh"
            script.write_text(src)
            x = subprocess.run([BASH, str(script), *(["--yes"] if yes else [])],
                env={**os.environ, "SOJU_BASE": str(base), "ENGINE": str(engine)},
                capture_output=True, text=True, timeout=15, start_new_session=True)
            self.assertEqual(x.returncode, 0, x.stderr)
            return {"stopped": marker.exists(), "bottle": (base / "bottle").exists(),
                    "engine": engine.exists(), "download": (base / "test.part").exists(),
                    "base": base.exists()}

    def test_reject_all_has_no_effect(self):
        x = self.scenario("n\nn\nn\n")
        self.assertFalse(x["stopped"])
        self.assertTrue(x["bottle"] and x["engine"] and x["download"])

    def test_no_terminal_has_no_effect(self):
        x = self.scenario()
        self.assertFalse(x["stopped"])
        self.assertTrue(x["bottle"] and x["engine"] and x["download"])

    def test_empty_directory_preserved_without_consent(self):
        self.assertTrue(self.scenario(empty=True)["base"])

    def test_accept_bottle_only(self):
        x = self.scenario("y\nn\nn\n")
        self.assertTrue(x["stopped"])
        self.assertFalse(x["bottle"])
        self.assertTrue(x["engine"] and x["download"])

    def test_yes_still_removes_everything(self):
        x = self.scenario(yes=True)
        self.assertTrue(x["stopped"])
        self.assertFalse(x["base"])


class HelperTests(unittest.TestCase):
    def test_build_failure_retry_and_existing_helper(self):
        with tempfile.TemporaryDirectory(prefix="soju-test-") as t:
            d = Path(t); bin_dir = d / "bin"; bin_dir.mkdir()
            compiler = bin_dir / "x86_64-w64-mingw32-gcc"
            compiler.write_text("""#!/usr/bin/python3
import sys, os
from pathlib import Path
out = Path(sys.argv[sys.argv.index('-o')+1])
out.write_bytes(b'MZfixture')
sys.exit(7 if os.environ.get('FAIL_BUILD') == '1' else 0)
""")
            compiler.chmod(0o755)
            env = {**os.environ, "PATH": str(bin_dir)+":"+os.environ["PATH"]}
            script = ROOT / "scripts/ensure-launcher-helper.sh"
            for mode in ("epic", "gog"):
                support = d / mode
                dest = support / ("soju-"+mode+"-restore.exe")
                cmd = [BASH, str(script), mode, str(support)]
                x = subprocess.run(cmd, env={**env, "FAIL_BUILD": "1"}, capture_output=True)
                self.assertNotEqual(x.returncode, 0)
                self.assertFalse(dest.exists())
                self.assertEqual(list(support.iterdir()), [])
                x = subprocess.run(cmd, env=env, capture_output=True)
                self.assertEqual(x.returncode, 0, x.stderr)
                self.assertGreater(dest.stat().st_size, 0)
                x = subprocess.run(cmd, env={**env, "FAIL_BUILD": "1"}, capture_output=True)
                self.assertEqual(x.returncode, 0)
                dest.write_bytes(b"")
                x = subprocess.run(cmd, env=env, capture_output=True)
                self.assertEqual(x.returncode, 0)
                self.assertGreater(dest.stat().st_size, 0)

    def test_installer_repairs_helpers_for_existing_clients(self):
        src = (ROOT / "install.sh").read_text()
        stage = src[src.index('ALL="battlenet steam epic gog"'):
                    src.index("# ---------- 4. App bundles")]
        with tempfile.TemporaryDirectory(prefix="soju-test-") as t:
            d = Path(t)
            for path in (
                "epic-bottle/drive_c/Program Files/Epic Games/Launcher/Portal/Binaries/Win64/EpicGamesLauncher.exe",
                "gog-bottle/drive_c/Program Files/GOG Galaxy/GalaxyClient.exe"):
                p=d/path; p.parent.mkdir(parents=True); p.touch()
            (d/"scripts").mkdir()
            (d/"scripts/ensure-launcher-helper.sh").write_text('echo "REPAIR $1"\n')
            pre = ('set -euo pipefail\nBASE='+shlex.quote(t)+
                   '\nBOTTLE="$BASE/bottle"\nENGINE="$BASE/engine"\nSOJU_DIR="$BASE"'
                   '\nSOJU_PLATFORMS=epic,gog\nsay(){ echo "$*"; }\n')
            x = run(pre+stage)
            self.assertEqual(x.returncode, 0, x.stderr)
            self.assertIn("REPAIR epic", x.stdout)
            self.assertIn("REPAIR gog", x.stdout)

    def test_noninteractive_selection_reaches_bash(self):
        x = run("printf x | SOJU_PLATFORMS=epic,gog /bin/bash -c 'echo \"$SOJU_PLATFORMS\"'")
        self.assertEqual(x.stdout.strip(), "epic,gog")


if __name__ == "__main__":
    unittest.main()
