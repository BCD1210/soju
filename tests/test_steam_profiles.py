import importlib.util
import json
import os
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def load(prefix):
    os.environ["WINEPREFIX"] = str(prefix)
    spec = importlib.util.spec_from_file_location("steam_profiles", ROOT / "scripts/steam-profiles.py")
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


def pe64(path, size=4096):
    data = bytearray(size)
    data[0:2] = b"MZ"
    data[0x3c:0x40] = (0x80).to_bytes(4, "little")
    data[0x80:0x86] = b"PE\0\0\x64\x86"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(bytes(data))


class Profiles(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.prefix = Path(self.tmp.name)
        apps = self.prefix / "drive_c/Program Files (x86)/Steam/steamapps"
        apps.mkdir(parents=True)
        (apps / "appmanifest_374320.acf").write_text('"AppState"\n{\n\t"appid"\t\t"374320"\n\t"installdir"\t\t"DARK SOULS III"\n}\n')
        (apps / "appmanifest_10.acf").write_text('"AppState"\n{\n\t"appid"\t\t"10"\n\t"installdir"\t\t"Other"\n}\n')
        game = apps / "common/DARK SOULS III/Game"
        pe64(game / "DarkSoulsIII.exe", 8192)
        pe64(game / "UnityCrashHandler64.exe", 16384)   # bigger, but a helper
        pe64(apps / "common/Other/other.exe")

    def tearDown(self):
        self.tmp.cleanup()
        os.environ.pop("WINEPREFIX", None)

    def test_picks_known_game_and_its_main_exe(self):
        todo, _, _ = load(self.prefix).plan()
        self.assertEqual([(a, e.name) for a, _, e in todo], [("374320", "DarkSoulsIII.exe")])

    def test_player_choice_is_kept(self):
        (self.prefix / ".soju-auto-profiles.json").write_text(json.dumps({"374320": {"renderer": "dxvk"}}))
        self.assertEqual(load(self.prefix).plan()[0], [])

    def test_profiles_are_d3d11_dxvk_only(self):
        data = json.loads((ROOT / "resources/steam-profiles.json").read_text())["profiles"]
        self.assertTrue(data)
        self.assertTrue(all(p["renderer"] == "dxvk" and p["why"] for p in data.values()))


if __name__ == "__main__":
    unittest.main()
