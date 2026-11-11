"""EA app, Rockstar Games Launcher and Ubisoft Connect: install plan, component pin, wiring."""
import json
import os
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parent.parent


class LauncherTests(unittest.TestCase):
    def test_plan_needs_the_engine(self):
        env = dict(os.environ, SOJU_PLATFORMS="ea,rockstar,ubisoft")
        out = subprocess.run(["bash", str(ROOT / "install.sh"), "--plan"], env=env,
                             capture_output=True, text=True, check=True).stdout
        self.assertIn("platforms=ea rockstar ubisoft", out)
        self.assertIn("cx_engine=1", out)
        self.assertIn("steam_runtime=0", out)

    def test_component_pin(self):
        m = json.loads((ROOT / "resources/launcher-support.json").read_text())
        self.assertTrue(m["url"].startswith("https://github.com/BCD1210/soju/releases/download/" + m["version"] + "/"))
        self.assertRegex(m["sha256"], r"^[0-9a-f]{64}$")
        for name in ("winemac.so", "dxmt/x86_64-unix/winemetal.so", "SOURCES.txt"):
            self.assertIn(name, m["files"])
        for arch in ("x86_64-windows", "i386-windows"):
            for dll in ("d3d10core", "d3d11", "dxgi", "winemetal"):
                self.assertIn("dxmt/%s/%s.dll" % (arch, dll), m["files"])

    def test_play_modes_are_wired(self):
        play = (ROOT / "scripts/play.sh").read_text()
        cli = (ROOT / "scripts/soju").read_text()
        reaper = (ROOT / "scripts/soju-reaper.sh").read_text()
        for mode in ("ea", "rockstar", "ubisoft"):
            self.assertIn(mode + "-kill", play)
            self.assertIn(mode + "-install", cli)
            self.assertIn("  %s)\n" % mode, reaper)


if __name__ == "__main__":
    unittest.main()
