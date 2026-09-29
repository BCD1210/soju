"""Game renderer changes use temporary prefixes; never launch installed games."""
import importlib.util
import io
import json
from pathlib import Path
import struct
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("steam_renderer", ROOT / "scripts/steam-renderer.py")
r = importlib.util.module_from_spec(spec)
spec.loader.exec_module(r)


def pe(path, machine=b"\x64\x86"):
    data = bytearray(128)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 0x3c, 64)
    data[64:70] = b"PE\0\0" + machine
    path.write_bytes(data)


class RendererTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.base = Path(temp.name)
        self.prefix = self.base / "steam-bottle"
        self.runtime = self.base / "steam-runtime"
        self.prefix.mkdir(); self.runtime.mkdir()
        (self.runtime / ".soju-runtime").write_text("v1")
        (self.prefix / ".soju-steam-games").write_text("v1")
        self.game = self.prefix / "Dark Souls III" / "DarkSoulsIII.exe"
        self.game.parent.mkdir(); pe(self.game)
        self.registry = self.prefix / "user.reg"
        self.original = ('WINE REGISTRY Version 2\n\n'
                         '[Software\\\\Wine\\\\DllOverrides]\n"d3d11"="builtin"\n\n'
                         '[Software\\\\Wine\\\\AppDefaults\\\\steam.exe\\\\DllOverrides]\n'
                         '"dxgi"="native"\n"winemetal"="disabled"\n\n')
        self.registry.write_text(self.original)
        self.payload = self.base / "payload"; self.payload.mkdir()
        for name in r.FILES: (self.payload / name).write_bytes(("DXVK-" + name).encode())
        idle = patch.object(r.session, "require_idle")
        fetch = patch.object(r, "fetch_dxvk", return_value=self.payload)
        self.idle = idle.start(); self.fetch = fetch.start()
        self.addCleanup(idle.stop); self.addCleanup(fetch.stop)

    def configure(self, mode, game=None):
        return r.configure(self.base, self.prefix, self.runtime, game or self.game, mode)

    def test_switch_both_directions_and_reset_preserves_custom_values_and_files(self):
        original_values = {"d3d11": "native,builtin", "d3d10core": None,
                           "dxgi": "builtin", "winemetal": ""}
        text = r.with_overrides(self.original, self.game.name, original_values)
        text += '"unrelated"="keep"\n'
        self.registry.write_text(text)
        (self.game.parent / "d3d11.dll").write_bytes(b"original plugin")
        self.configure("dxvk")
        self.assertEqual(r.overrides(self.registry.read_text(), self.game.name)["dxgi"], "native")
        self.assertEqual((self.game.parent / "d3d11.dll").read_bytes(), b"DXVK-d3d11.dll")
        self.assertTrue(self.registry.read_text().startswith(self.original))
        self.configure("dxmt")
        self.assertEqual(r.overrides(self.registry.read_text(), self.game.name), dict.fromkeys(r.DLLS, "builtin"))
        self.assertEqual((self.game.parent / "d3d11.dll").read_bytes(), b"original plugin")
        self.assertFalse((self.game.parent / "d3d10core.dll").exists())
        self.configure("dxvk")
        self.configure("reset")
        self.assertEqual(r.overrides(self.registry.read_text(), self.game.name), original_values)
        self.assertIn('"unrelated"="keep"', self.registry.read_text())
        self.assertEqual((self.game.parent / "d3d11.dll").read_bytes(), b"original plugin")
        self.assertFalse(list((self.prefix / ".soju-renderers").glob("*.json")))

    def test_status_and_no_profile_reset_leave_files_unchanged(self):
        self.assertEqual(self.configure("status")["renderer"], "unmanaged")
        self.idle.assert_not_called(); self.fetch.assert_not_called()
        self.configure("reset")
        self.assertEqual(self.registry.read_text(), self.original)
        self.assertFalse((self.prefix / ".soju-renderers").exists())

    def test_live_steam_blocks_change_before_download(self):
        self.idle.side_effect = RuntimeError("Close Windows Steam")
        with self.assertRaises(RuntimeError): self.configure("dxvk")
        self.fetch.assert_not_called()
        self.assertEqual(self.registry.read_text(), self.original)

    def test_steam_starting_during_download_blocks_change(self):
        self.idle.side_effect = [None, RuntimeError("Close Windows Steam")]
        with self.assertRaises(RuntimeError): self.configure("dxvk")
        self.assertEqual(self.registry.read_text(), self.original)

    def test_changed_files_or_registry_are_not_overwritten(self):
        self.configure("dxvk")
        (self.game.parent / "d3d11.dll").write_bytes(b"updated by game")
        with self.assertRaisesRegex(ValueError, "changed outside"): self.configure("reset")
        self.assertEqual((self.game.parent / "d3d11.dll").read_bytes(), b"updated by game")
        (self.game.parent / "d3d11.dll").write_bytes(b"DXVK-d3d11.dll")
        self.registry.write_text(r.with_overrides(self.registry.read_text(), self.game.name, dict.fromkeys(r.DLLS, "builtin")))
        with self.assertRaisesRegex(ValueError, "changed outside"): self.configure("dxmt")

    def test_duplicate_executable_name_or_shared_game_directory_is_rejected(self):
        self.configure("dxvk")
        other = self.prefix / "another" / self.game.name
        other.parent.mkdir(); pe(other)
        with self.assertRaisesRegex(ValueError, "executable name"): self.configure("dxvk", other)
        other = self.game.parent / "other.exe"; pe(other)
        with self.assertRaisesRegex(ValueError, "shared DLLs"): self.configure("dxvk", other)

    def test_links_and_case_aliases_are_not_replaced(self):
        target = self.base / "unrelated"; target.write_bytes(b"keep")
        dll = self.game.parent / "d3d11.dll"; dll.symlink_to(target)
        with self.assertRaises(ValueError): self.configure("dxvk")
        self.assertEqual(target.read_bytes(), b"keep")
        dll.unlink()
        (self.game.parent / "D3D11.dll").write_bytes(b"keep")
        with self.assertRaisesRegex(ValueError, "lowercase"): self.configure("dxvk")

    def test_steam_client_and_32_bit_game_are_rejected(self):
        steam = self.game.parent / "Steam.exe"; pe(steam)
        with self.assertRaises(ValueError): self.configure("dxvk", steam)
        pe(self.game, b"\x4c\x01")
        with self.assertRaisesRegex(ValueError, "x86-64"): self.configure("dxvk")

    def test_failure_restores_all_previous_game_and_registry_bytes(self):
        before = self.game.parent / "d3d11.dll"; before.write_bytes(b"original")
        real = r.atomic
        failed = False
        def fail_once(path, data):
            nonlocal failed
            if path == self.registry and not failed:
                failed = True
                raise OSError("disk failure")
            return real(path, data)
        with patch.object(r, "atomic", side_effect=fail_once):
            with self.assertRaises(OSError): self.configure("dxvk")
        self.assertEqual(self.registry.read_text(), self.original)
        self.assertEqual(before.read_bytes(), b"original")
        self.assertFalse((self.game.parent / "d3d10core.dll").exists())
        self.assertFalse(list((self.prefix / ".soju-renderers").glob("*.json")))
        self.configure("dxvk")  # failed first setup left no conflicting recovery directory

    def test_reset_works_with_missing_runtime(self):
        self.configure("dxvk")
        (self.runtime / ".soju-runtime").unlink()
        self.configure("reset")
        self.assertFalse((self.game.parent / "d3d11.dll").exists())

    def test_unprepared_bottle_is_not_modified(self):
        (self.prefix / ".soju-steam-games").unlink()
        with self.assertRaisesRegex(ValueError, "Prepare"): self.configure("dxvk")
        self.assertEqual(self.registry.read_text(), self.original)
        self.fetch.assert_not_called()

    def test_duplicate_or_non_string_registry_override_rejected(self):
        text = r.with_overrides(self.original, self.game.name, {"d3d11": "native"})
        self.registry.write_text(text + '"d3d11"="builtin"\n')
        with self.assertRaises(ValueError): self.configure("dxvk")
        self.registry.write_text(text.replace('"native"\n', 'dword:00000001\n'))
        with self.assertRaises(ValueError): self.configure("dxvk")


class DownloadTests(unittest.TestCase):
    def test_corrupt_download_does_not_install(self):
        with tempfile.TemporaryDirectory() as temp:
            def download(args, **kwargs):
                Path(args[-1]).write_bytes(b"corrupt")
            with patch.object(r.subprocess, "run", side_effect=download):
                with self.assertRaisesRegex(ValueError, "checksum"):
                    r.fetch_dxvk(Path(temp))
            self.assertFalse(any(Path(temp).rglob("d3d11.dll")))

    def test_verified_cache_needs_no_network_and_tampering_does(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            manifest = json.loads((ROOT / "resources/steam-dxvk.json").read_text())
            target = base / "steam-support" / ("dxvk-" + manifest["version"])
            target.mkdir(parents=True)
            for name in r.FILES: (target / name).write_bytes(name.encode())
            manifest["files"] = {name: r.signature(target / name) for name in r.FILES}
            original = Path.read_text
            def read(path, *args, **kwargs):
                return json.dumps(manifest) if path.name == "steam-dxvk.json" else original(path, *args, **kwargs)
            with patch.object(Path, "read_text", read), patch.object(r.subprocess, "run", side_effect=RuntimeError("network")) as run:
                self.assertEqual(r.fetch_dxvk(base), target)
                run.assert_not_called()
                (target / "d3d11.dll").write_bytes(b"tampered")
                with self.assertRaisesRegex(RuntimeError, "network"): r.fetch_dxvk(base)
