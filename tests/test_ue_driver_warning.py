import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts/ue_driver_warning.py"


class UnrealDriverWarningTests(unittest.TestCase):
    def test_unreal_games_that_ran_get_the_setting_once_and_launchers_are_left_alone(self):
        with tempfile.TemporaryDirectory() as d:
            local = Path(d) / "drive_c/users/crossover/AppData/Local"
            game = local / "HogwartsLegacy/Saved/Config/WindowsNoEditor"
            game.mkdir(parents=True)
            (game / "Engine.ini").write_bytes(b"[Core.System]\r\nPaths=../../Content\r\n\r\n[SystemSettings]\r\nr.Foo=1\r\n")
            other = local / "Seven/Saved/Config/Windows"
            other.mkdir(parents=True)
            launcher = local / "EpicGamesLauncher/Saved/Config/Windows"
            launcher.mkdir(parents=True)
            for _ in range(2):   # twice: the second run changes nothing
                subprocess.run([sys.executable, str(SCRIPT), d], check=True)
            text = (game / "Engine.ini").read_bytes()
            self.assertEqual(text.count(b"r.WarnOfBadDrivers=0"), 1)
            self.assertIn(b"[SystemSettings]\r\nr.WarnOfBadDrivers=0\r\nr.Foo=1\r\n", text)
            self.assertIn(b"[Core.System]\r\nPaths=../../Content\r\n", text)
            self.assertIn("[SystemSettings]\r\nr.WarnOfBadDrivers=0", (other / "Engine.ini").read_bytes().decode("utf-8"))
            self.assertFalse((launcher / "Engine.ini").exists())

    def test_play_sh_runs_it(self):
        self.assertIn("ue_driver_warning.py", (ROOT / "scripts/play.sh").read_text())
        self.assertIn("ue_driver_warning.py", (ROOT / "scripts/xbox-wine.sh").read_text())


if __name__ == "__main__":
    unittest.main()
