import os
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class UninstallLauncher(unittest.TestCase):
    def run_action(self, base, *args):
        env = {k: v for k, v in os.environ.items() if k not in ("ENGINE", "WINEPREFIX", "SOJU_STEAM_WINE")}
        env.update(SOJU_BASE=str(base), HOME=str(base / "home"))
        return subprocess.run(["/bin/bash", str(ROOT / "scripts/desktop-action.sh"), "uninstall", *args],
                              env=env, capture_output=True, text=True, timeout=30)

    def test_removes_only_that_launcher_and_keeps_moved_games(self):
        with tempfile.TemporaryDirectory() as t:
            base = Path(t)
            for b in ("gog-bottle/drive_c", "epic-bottle/drive_c", "bottle/drive_c"):
                (base / b).mkdir(parents=True)
            drive = base / "external"; drive.mkdir(); (drive / "game.bin").write_text("game")
            (base / "gog-bottle/drive_c/moved").symlink_to(drive)
            app = base / "home/Applications/GOG GALAXY.app/Contents/MacOS"; app.mkdir(parents=True)
            (app / "launcher").write_text('ENGINE="%s/cx26-engine"\nexec scripts/play.sh gog\n' % base)
            r = self.run_action(base, "launcher", "gog")
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
            self.assertFalse((base / "gog-bottle").exists())
            self.assertFalse((base / "home/Applications/GOG GALAXY.app").exists())
            self.assertTrue((base / "epic-bottle").is_dir() and (base / "bottle").is_dir())
            self.assertEqual((drive / "game.bin").read_text(), "game")
            again = self.run_action(base, "launcher", "gog")
            self.assertEqual(again.returncode, 0)
            self.assertIn("Nothing to remove", again.stdout)

    def test_unknown_launcher_is_refused(self):
        with tempfile.TemporaryDirectory() as t:
            base = Path(t); (base / "bottle").mkdir()
            r = self.run_action(base, "launcher", "../bottle")
            self.assertEqual(r.returncode, 64)
            self.assertTrue((base / "bottle").is_dir())


if __name__ == "__main__":
    unittest.main()
