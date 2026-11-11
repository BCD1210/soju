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

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("fetch_support", ROOT / "scripts/fetch-steam-support.py")
fetcher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fetcher)

class InstallerPlanTests(unittest.TestCase):
    def plan(self, choices):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp) / "untouched"
            result = subprocess.run(["/bin/bash", str(ROOT / "install.sh"), "--plan"],
                env={**os.environ, "SOJU_PLATFORMS": choices, "SOJU_BASE": str(base)},
                capture_output=True, text=True)
            self.assertFalse(base.exists(), "Selection must precede filesystem changes")
            return result

    def test_steam_needs_no_cx_or_gptk(self):
        result = self.plan("steam")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("cx_engine=0", result.stdout)
        self.assertIn("apple_gptk=0", result.stdout)
        self.assertIn("steam_runtime=1", result.stdout)

    def test_mixed_selection_requests_both_runtimes(self):
        result = self.plan("steam,epic")
        self.assertEqual(result.returncode, 0)
        self.assertIn("cx_engine=1", result.stdout)
        self.assertIn("steam_runtime=1", result.stdout)

    def test_cx_selection_skips_steam_runtime(self):
        result = self.plan("battlenet,epic,gog")
        self.assertEqual(result.returncode, 0)
        self.assertIn("steam_runtime=0", result.stdout)

    def test_invalid_selection_has_no_side_effects(self):
        self.assertNotEqual(self.plan("steam,unknown").returncode, 0)
        self.assertNotEqual(self.plan("   ").returncode, 0)

class ArtifactTests(unittest.TestCase):
    data = b"MZ-test-component"
    def manifest(self):
        return {"version": "test-v1", "files": {"dxmt-x64/d3d11.dll": hashlib.sha256(self.data).hexdigest()}}

    def archive(self, path, entries):
        with tarfile.open(path, "w:gz") as tar:
            for name, kind in entries:
                item = tarfile.TarInfo(name)
                if kind == "link":
                    item.type = tarfile.SYMTYPE; item.linkname = "/tmp/escape"
                    tar.addfile(item)
                else:
                    item.size = len(self.data)
                    tar.addfile(item, io.BytesIO(self.data))

    def test_extracts_only_verified_files(self):
        with tempfile.TemporaryDirectory() as t:
            d = Path(t); dest = d/"out"; dest.mkdir()
            self.archive(d/"test.tgz", [("dxmt-x64/d3d11.dll", "file")])
            fetcher.unpack(d/"test.tgz", dest, self.manifest())
            self.assertTrue(fetcher.verified(dest, self.manifest()))

    def test_rejects_traversal_links_duplicates_and_unknown_payloads(self):
        cases = [
            [("../escape", "file")],
            [("/tmp/escape", "file")],
            [("dxmt-x64/d3d11.dll", "link")],
            [("dxmt-x64/d3d11.dll", "file"), ("dxmt-x64/d3d11.dll", "file")],
            [("unapproved.exe", "file")],
        ]
        for entries in cases:
            with self.subTest(entries=entries), tempfile.TemporaryDirectory() as t:
                d=Path(t); (d/"out").mkdir()
                self.archive(d/"test.tgz", entries)
                with self.assertRaises(ValueError):
                    fetcher.unpack(d/"test.tgz", d/"out", self.manifest())

    def test_rejects_incomplete_or_corrupt_payload(self):
        with tempfile.TemporaryDirectory() as t:
            d=Path(t); (d/"out").mkdir()
            self.archive(d/"test.tgz", [])
            with self.assertRaises(ValueError):
                fetcher.unpack(d/"test.tgz", d/"out", self.manifest())
            self.archive(d/"test.tgz", [("dxmt-x64/d3d11.dll", "file")])
            m=self.manifest(); m["files"]["dxmt-x64/d3d11.dll"]="0"*64
            with self.assertRaises(ValueError):
                fetcher.unpack(d/"test.tgz", d/"out", m)

    def test_bad_download_preserves_previous_installation(self):
        with tempfile.TemporaryDirectory() as t:
            base=Path(t); target=base/"steam-support/prebuilt"
            target.mkdir(parents=True); (target/"keep.txt").write_text("old installation")
            m={**self.manifest(), "url":"https://example.invalid/support.tgz", "sha256":"0"*64}
            def download(args, **kwargs):
                Path(args[-1]).write_bytes(b"truncated archive")
            with patch.object(fetcher.subprocess, "run", side_effect=download):
                with self.assertRaises(ValueError): fetcher.fetch(base,m)
            self.assertEqual((target/"keep.txt").read_text(),"old installation")
            self.assertEqual(list((base/"steam-support").iterdir()),[target])

    def test_valid_cache_is_offline_and_detects_tampering(self):
        with tempfile.TemporaryDirectory() as t:
            base=Path(t); target=base/"steam-support/prebuilt"
            (target/"dxmt-x64").mkdir(parents=True)
            p=target/"dxmt-x64/d3d11.dll"; p.write_bytes(self.data)
            with patch.object(fetcher.subprocess,"run",side_effect=AssertionError("network")):
                self.assertEqual(fetcher.fetch(base,self.manifest()),target)
            p.write_bytes(b"tampered")
            self.assertFalse(fetcher.verified(target,self.manifest()))

class RecoveryTests(unittest.TestCase):
    def recover(self, ready):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        d=Path(temp.name); prefix=d/"prefix"; backup=d/"backup"; new=d/"new"
        system=prefix/"drive_c/windows/system32"
        system.mkdir(parents=True); backup.mkdir(); (new/"bin").mkdir(parents=True)
        (new/"bin/wineserver").write_text("#!/bin/bash\nexit 0\n")
        (new/"bin/wineserver").chmod(0o755)
        (prefix/"user.reg").write_text("changed")
        (system/"d3d11.dll").write_text("changed")
        if ready:
            (backup/"user.reg").write_text("original settings")
            (backup/"d3d11.dll").write_text("original DLL")
            (backup/"renderer-version").write_text("previous renderer")
        script=(ROOT/"scripts/setup-steam-games.sh").read_text()
        recovery=script[script.index("rollback(){"):script.index("trap rollback EXIT")]
        subprocess.run(["/bin/bash", "-c", recovery+"\nfalse; rollback"], check=True,
            env={**os.environ, "NEW":str(new), "BACKUP":str(backup), "BACKUP_READY":str(ready),
                 "WINEPREFIX":str(prefix), "B":str(prefix/"drive_c/windows")}, capture_output=True)
        return prefix, system

    def test_failed_setup_restores_settings_and_renderer(self):
        prefix,system=self.recover(1)
        self.assertEqual((prefix/"user.reg").read_text(),"original settings")
        self.assertEqual((system/"d3d11.dll").read_text(),"original DLL")
        self.assertEqual((prefix/".soju-steam-games").read_text(),"previous renderer")

    def test_failed_backup_does_not_delete_original_files(self):
        prefix,system=self.recover(0)
        self.assertEqual((prefix/"user.reg").read_text(),"changed")
        self.assertEqual((system/"d3d11.dll").read_text(),"changed")

class BattleNetSetupTests(unittest.TestCase):
    def install(self, stalls, agent_works=True):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        d = Path(temp.name); engine = d/"engine"; work = d/"work"; prefix = d/"bottle"; bin_ = d/"bin"
        for path in (engine/"bin", engine/"lib/external", work, bin_):
            path.mkdir(parents=True)
        (engine/"lib/external/libd3dshared.dylib").write_text("")
        (work/"Battle.net-Setup.exe").write_text("cached installer")
        stale = prefix/"drive_c/ProgramData/Battle.net/Agent"
        stale.mkdir(parents=True); (stale/"state").write_text("unfinished")
        bn = '"$WINEPREFIX/drive_c/Program Files (x86)/Battle.net"'
        tools = {
            engine/"bin/wineserver": "exit 0\n",
            engine/"bin/wine": f"""echo "$*" >> "{d}/wine.log"
case "$1" in
  wineboot|reg) exit 0;;
  *Agent.exe) exit 0;;
  cscript) {"mkdir -p " + bn + " && touch " + bn + "/Battle.net.exe" if agent_works else "exit 1"}; exit 0;;
esac
n=$(( $(cat "{d}/attempts" 2>/dev/null || echo 0) + 1 )); echo $n > "{d}/attempts"
mkdir -p "$WINEPREFIX/drive_c/ProgramData/Battle.net/Setup" "$WINEPREFIX/drive_c/ProgramData/Battle.net/Agent/Agent.9824"
touch "$WINEPREFIX/drive_c/ProgramData/Battle.net/Agent/Agent.9824/Agent.exe"
[ $n -le {stalls} ] && exec sleep 30
mkdir -p {bn}
touch {bn}/Battle.net.exe
""",
            bin_/"curl": f"""while [ $# -gt 0 ]; do [ "$1" = -o ] && out="$2"; shift; done
echo fresh > "$out"; echo x >> "{d}/downloads"
""",
            bin_/"stat": "echo 16777234 245634352\n",
            bin_/"pgrep": "echo 4242\n",
            bin_/"lsof": """case "$*" in
  *cwd*) echo p4242; echo n/tmp/.wine-501/server-1000012-ea41530;;
  *LISTEN*) echo p4242; echo n127.0.0.1:55475;;
esac
""",
        }
        for path, body in tools.items():
            path.write_text("#!/bin/bash\n" + body); path.chmod(0o755)
        result = subprocess.run(["/bin/bash", str(ROOT/"scripts/create-bottle.sh")], capture_output=True, text=True,
            env={**os.environ, "ENGINE": str(engine), "WORK": str(work), "WINEPREFIX": str(prefix),
                 "SETUP_TIMEOUT": "1", "PATH": f"{bin_}:{os.environ['PATH']}"}, timeout=120)
        attempts = int((d/"attempts").read_text()) if (d/"attempts").exists() else 0
        downloads = len((d/"downloads").read_text().split()) if (d/"downloads").exists() else 0
        self.wine_log = (d/"wine.log").read_text() if (d/"wine.log").exists() else ""
        return result, prefix, work, attempts, downloads

    def test_stalled_installer_is_finished_through_the_agent(self):
        result, prefix, work, attempts, downloads = self.install(stalls=1)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue((prefix/"drive_c/Program Files (x86)/Battle.net/Battle.net.exe").exists())
        self.assertEqual((attempts, downloads), (1, 0))
        self.assertIn("through the Battle.net Agent (Agent.9824)", result.stdout)
        self.assertNotIn("Retrying", result.stdout)
        self.assertIn("C:\\ProgramData\\Battle.net\\Agent\\Agent.9824\\Agent.exe --internalclienttools", self.wine_log)
        js = "Z:" + str(ROOT/"scripts/bnet-agent-install.js").replace("/", "\\")
        self.assertIn(f"cscript //nologo {js} 55475 C:\\Program Files (x86)\\Battle.net", self.wine_log)

    def test_stalled_installer_retries_once_from_clean_state_when_the_agent_fails(self):
        result, prefix, work, attempts, downloads = self.install(stalls=1, agent_works=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue((prefix/"drive_c/Program Files (x86)/Battle.net/Battle.net.exe").exists())
        self.assertEqual((attempts, downloads), (2, 1))
        self.assertIn("Retrying once", result.stdout)
        kept = sorted(p.read_text() for p in work.glob("previous-setup-*/Battle.net/Agent/state"))
        self.assertEqual(kept, ["unfinished"])
        self.assertEqual((work/"Battle.net-Setup.exe").read_text().strip(), "fresh")

    def test_installer_that_stalls_twice_fails_instead_of_hanging(self):
        result, prefix, work, attempts, downloads = self.install(stalls=2, agent_works=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("stalled again", result.stdout)
        self.assertEqual(attempts, 2)
        self.assertFalse((prefix/"drive_c/Program Files (x86)/Battle.net/Battle.net.exe").exists())

    def test_clean_install_runs_once_without_moving_anything(self):
        result, prefix, work, attempts, downloads = self.install(stalls=0)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual((attempts, downloads), (1, 0))
        self.assertNotIn("Retrying", result.stdout)
        self.assertNotIn("Agent", result.stdout)

    def test_agent_install_script_makes_the_setup_calls(self):
        js = (ROOT/"scripts/bnet-agent-install.js").read_text()
        for needle in ('"phoenix-agent/1.0"', '"/agent"', '"/install"', '"/install/battle.net"',
                       '"finalized":true', 'WinHttp.WinHttpRequest.5.1'):
            self.assertIn(needle, js)

class SupportManifestTests(unittest.TestCase):
    def test_published_manifest_names_one_release(self):
        m = json.loads((ROOT/"resources/steam-support.json").read_text())
        base = "https://github.com/BCD1210/soju/releases/download/" + m["version"] + "/soju-" + m["version"]
        self.assertEqual(m["url"], base + ".tar.gz")
        self.assertEqual(m["source_url"], base + "-sources.tar.gz")
        self.assertRegex(m["sha256"], "^[0-9a-f]{64}$")
        self.assertTrue(all(len(v) == 64 for v in m["files"].values()))

    def test_every_dxmt_patch_is_applied_by_the_build(self):
        script = (ROOT/"scripts/build-steam-support.sh").read_text()
        for patch in ("steam-dxmt-build.patch", "dxmt-max-tess-factor.patch", "dxmt-nonfatal-pure-virtual.patch"):
            self.assertTrue((ROOT/"patches"/patch).is_file())
            self.assertIn('apply_once dxmt "$ROOT/patches/' + patch + '"', script)
        self.assertIn("d3d11.maxTessFactor", (ROOT/"patches/dxmt-max-tess-factor.patch").read_text())
        # Skyrim SE releases a destroyed texture twice; the handler must log and return, not trap.
        pure_virtual = (ROOT/"patches/dxmt-nonfatal-pure-virtual.patch").read_text()
        self.assertIn("+#ifndef NDEBUG\n   __builtin_trap();\n+#endif", pure_virtual)

    def test_surround_audio_patches_are_built_and_installed(self):
        # #93: 5.1/7.1 output lost centre and back on stereo Macs in the Steam runtime.
        script = (ROOT/"scripts/build-steam-support.sh").read_text()
        for patch in ("steam-winecoreaudio-io-buffer.patch",
                      "highball/0022-mmdevapi-spatial-static-object-order.patch",
                      "highball/0023-winecoreaudio-spatial-mixer-for-surround-beds.patch"):
            self.assertTrue((ROOT/"patches"/patch).is_file())
            self.assertIn('apply_once wine "$ROOT/patches/' + patch + '"', script)
        for out in ("output/winecoreaudio-patched.so", "output/mmdevapi-x64.dll"):
            self.assertIn(out, script)
        setup = (ROOT/"scripts/setup-steam-games.sh").read_text()
        self.assertIn('"$SUP/winecoreaudio-patched.so" "$WLIB/x86_64-unix/winecoreaudio.so"', setup)
        self.assertIn('"$SUP/mmdevapi-x64.dll" "$WLIB/x86_64-windows/mmdevapi.dll"', setup)
        files = json.loads((ROOT/"resources/steam-support.json").read_text())["files"]
        self.assertIn("winecoreaudio-patched.so", files)
        self.assertIn("mmdevapi-x64.dll", files)

class EnginePatchTests(unittest.TestCase):
    def test_stored_acls_patch_is_applied_once_by_the_engine_build(self):
        script = (ROOT/"scripts/build-engine.sh").read_text()
        patch = (ROOT/"patches/wineserver-stored-acls.patch").read_text()
        # The guard string the build checks for must be one the patch adds.
        self.assertIn('grep -q "WINE_XATTR_SD" "$WINE/server/file.c"', script)
        self.assertIn('patches/wineserver-stored-acls.patch', script)
        self.assertIn('+#define WINE_XATTR_SD "user.wine.sd"', patch)
        for path in ("server/file.c", "server/file.h", "server/change.c"):
            self.assertIn("+++ b/" + path, patch)

class DownloadCheckTests(unittest.TestCase):
    """install.sh and update.sh run only checked downloads, never main."""
    SCRIPTS = ("install.sh", "scripts/update.sh")

    def shell_function(self, script, name):
        text = (ROOT/script).read_text()
        start = text.index(name + "(){")
        return text[start:text.index("\n}\n", start) + 3]

    def run_bash(self, code, env=None):
        return subprocess.run(["/bin/bash", "-c", code], capture_output=True, text=True,
                              env={**os.environ, **(env or {})})

    def test_engine_pin_is_the_same_everywhere(self):
        import re
        pins = set()
        for script in self.SCRIPTS:
            text = (ROOT/script).read_text()
            tag = re.search(r'^ENGINE_TAG="([^"]+)"$', text, re.M).group(1)
            sha = re.search(r'^ENGINE_SHA256="([0-9a-f]{64})"$', text, re.M).group(1)
            pins.add((tag, sha))
        self.assertEqual(len(pins), 1, pins)

    def test_no_unchecked_downloads_of_code(self):
        for script in self.SCRIPTS + ("scripts/get-components.sh", "scripts/build-engine.sh"):
            text = (ROOT/script).read_text()
            self.assertNotIn("refs/heads/main", text, script)
            self.assertNotIn("browser_download_url", text, script)
            self.assertNotRegex(text, r"curl [^\n]*\| *tar", script)

    def test_helpers_are_identical_in_both_scripts(self):
        for name in ("fetch_verified", "latest_release"):
            a, b = (self.shell_function(s, name) for s in self.SCRIPTS)
            self.assertEqual(a, b, name)

    def test_fetch_verified_keeps_only_matching_files(self):
        fn = self.shell_function("install.sh", "fetch_verified")
        with tempfile.TemporaryDirectory() as t:
            src = Path(t)/"src.bin"; src.write_bytes(b"soju")
            good = hashlib.sha256(b"soju").hexdigest()
            ok = self.run_bash(fn + f'fetch_verified "file://{src}" "{t}/ok" {good}')
            self.assertEqual(ok.returncode, 0, ok.stdout + ok.stderr)
            self.assertEqual((Path(t)/"ok").read_bytes(), b"soju")
            bad = self.run_bash(fn + f'fetch_verified "file://{src}" "{t}/bad" {"0" * 64}')
            self.assertNotEqual(bad.returncode, 0)
            self.assertIn("Checksum mismatch", bad.stdout)
            self.assertFalse((Path(t)/"bad").exists())
            self.assertFalse((Path(t)/"bad.part").exists())

    def latest(self, releases):
        fn = self.shell_function("install.sh", "latest_release")
        with tempfile.TemporaryDirectory() as t:
            fixture = Path(t)/"releases.json"; fixture.write_text(json.dumps(releases))
            fake_curl = f'curl(){{ cp "{fixture}" "${{@: -1}}"; }}\n'
            return self.run_bash('REPO=BCD1210/soju\n' + fake_curl + fn + "latest_release")

    def test_latest_release_picks_newest_stable_app_zip_with_digest(self):
        d = "a" * 64
        result = self.latest([
            {"tag_name": "engine-v9", "prerelease": False, "assets": [{"name": "wine-engine-x86_64.tar.xz", "digest": "sha256:" + d}]},
            {"tag_name": "v2.0.0", "prerelease": True, "assets": [{"name": "Soju-2.0.0-macos-arm64.zip", "digest": "sha256:" + d}]},
            {"tag_name": "v1.9.0", "prerelease": False, "assets": [{"name": "Soju-1.9.0-macos-arm64.zip", "digest": None}]},
            {"tag_name": "v1.9.1", "prerelease": False, "assets": [
                {"name": "Soju-1.9.1-macos-arm64.zip.sha256", "digest": "sha256:" + "b" * 64},
                {"name": "Soju-1.9.1-macos-arm64.zip", "digest": "sha256:" + d}]},
        ])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.split(), ["v1.9.1",
            "https://github.com/BCD1210/soju/releases/download/v1.9.1/Soju-1.9.1-macos-arm64.zip", d])

    def test_latest_release_fails_without_a_checked_zip(self):
        result = self.latest([{"tag_name": "v1.0.0", "prerelease": False, "assets": []}])
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")

    def test_build_downloads_are_pinned(self):
        for script, count in (("scripts/get-components.sh", 2), ("scripts/build-engine.sh", 2),
                              ("scripts/build-steam-support.sh", 1)):
            text = (ROOT/script).read_text()
            self.assertEqual(text.count("fetch_checked \"$") + text.count("fetch_checked https"), count, script)
            self.assertNotIn(".part\" && mv", text, script)

if __name__ == "__main__":
    unittest.main()
