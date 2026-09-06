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

if __name__ == "__main__":
    unittest.main()
