import os
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts/own-d3dcompiler.sh"
KEY = "HKCU\\Software\\Wine\\AppDefaults\\DiabloImmortal.exe\\DllOverrides"


class OwnD3DCompilerTests(unittest.TestCase):
    def run_script(self, user_reg=None):
        tmp = tempfile.TemporaryDirectory(); self.addCleanup(tmp.cleanup)
        root = Path(tmp.name); bottle = root / "bottle"; engine = root / "engine"
        (engine / "bin").mkdir(parents=True); bottle.mkdir()
        calls = root / "calls"
        wine = engine / "bin/wine"
        wine.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> "' + str(calls) + '"\n'); wine.chmod(0o755)
        if user_reg is not None:
            (bottle / "user.reg").write_text(user_reg)
        env = dict(os.environ, WINEPREFIX=str(bottle), ENGINE=str(engine))
        done = subprocess.run(["bash", str(SCRIPT)], env=env, capture_output=True, text=True)
        self.assertEqual(done.returncode, 0, done.stderr)
        return calls.read_text().splitlines() if calls.exists() else []

    def test_adds_the_games_own_compilers_once(self):
        expected = ["reg add " + KEY + " /v " + dll + " /t REG_SZ /d native /f" for dll in ("d3dcompiler_47", "d3dcompiler_43")]
        self.assertEqual(self.run_script(), expected)
        self.assertEqual(self.run_script("WINE REGISTRY Version 2\n\n[Software\\\\Wine\\\\Fonts] 1\n"), expected)

    def test_an_existing_key_is_left_alone_and_wine_is_not_started(self):
        reg = ("WINE REGISTRY Version 2\n\n[Software\\\\Wine\\\\AppDefaults\\\\DiabloImmortal.exe\\\\DllOverrides] 1795672426\n"
               '"d3dcompiler_47"="builtin"\n')
        self.assertEqual(self.run_script(reg), [])

    def test_no_engine_or_bottle_is_not_an_error(self):
        done = subprocess.run(["bash", str(SCRIPT)], env={"PATH": os.environ["PATH"]}, capture_output=True, text=True)
        self.assertEqual(done.returncode, 0)

    def test_battle_net_starts_run_it(self):
        text = (ROOT / "scripts/play.sh").read_text()
        battlenet = text[text.index("  battlenet)   #"):text.index("  d2r)   ")]
        self.assertIn("own-d3dcompiler.sh", battlenet)


if __name__ == "__main__":
    unittest.main()
