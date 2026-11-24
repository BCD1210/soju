import os
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts/gptk-dlss.sh"


def fake_engine(base, shim=True):
    eng = Path(base) / "engine"
    (eng / "lib/wine/x86_64-windows").mkdir(parents=True)
    (eng / "lib/wine/x86_64-unix").mkdir(parents=True)
    (eng / "lib/external").mkdir(parents=True)
    (eng / "lib/external/libd3dshared.dylib").write_bytes(b"dylib")
    if shim:
        (eng / "lib/wine/x86_64-windows/nvngx-on-metalfx.dll").write_bytes(b"apple shim")
    (eng / "bin").mkdir()
    wine = eng / "bin/wine"   # records its arguments instead of running Windows code
    wine.write_text('#!/bin/sh\necho "$@" >> "$WINEPREFIX/wine-calls"\n'
                    'printf \'\\n[Software\\\\\\\\Nvidia Corporation\\\\\\\\Global\\\\\\\\NGXCore] 1\\n\' >> "$WINEPREFIX/system.reg"\n')
    wine.chmod(0o755)
    return eng


def run(eng, prefix):
    return subprocess.run(["bash", str(SCRIPT), str(eng), str(prefix)]).returncode


class GptkDlssTests(unittest.TestCase):
    def test_engine_gets_nvngx_and_symlinked_unixlib_and_prefix_gets_ngxcore_once(self):
        with tempfile.TemporaryDirectory() as d:
            eng = fake_engine(d)
            prefix = Path(d) / "bottle"; prefix.mkdir()
            (prefix / "system.reg").write_text("WINE REGISTRY Version 2\n")
            sys32 = prefix / "drive_c/windows/system32"; sys32.mkdir(parents=True)
            (sys32 / "nvngx.dll").write_bytes(b"wine placeholder")
            for _ in range(2):   # the second run changes nothing
                self.assertEqual(run(eng, prefix), 0)
            self.assertEqual((eng / "lib/wine/x86_64-windows/nvngx.dll").read_bytes(), b"apple shim")
            self.assertEqual((sys32 / "nvngx.dll").read_bytes(), b"apple shim")
            so = eng / "lib/wine/x86_64-unix/nvngx.so"
            # A copied unixlib looks for D3DMetal next to itself and aborts the process.
            self.assertTrue(so.is_symlink())
            self.assertEqual(os.readlink(so), "../../external/libd3dshared.dylib")
            calls = (prefix / "wine-calls").read_text().splitlines()
            self.assertEqual(len(calls), 1)
            self.assertIn("NGXCore", calls[0])
            self.assertIn("FullPath", calls[0])

    def test_a_copied_unixlib_is_replaced_by_the_symlink(self):
        with tempfile.TemporaryDirectory() as d:
            eng = fake_engine(d)
            prefix = Path(d) / "bottle"; prefix.mkdir()
            (prefix / "system.reg").write_text("x\n")
            (eng / "lib/wine/x86_64-unix/nvngx.so").write_bytes(b"copy")
            self.assertEqual(run(eng, prefix), 0)
            self.assertTrue((eng / "lib/wine/x86_64-unix/nvngx.so").is_symlink())

    def test_without_apples_shim_nothing_is_touched(self):
        with tempfile.TemporaryDirectory() as d:
            eng = fake_engine(d, shim=False)
            prefix = Path(d) / "bottle"; prefix.mkdir()
            (prefix / "system.reg").write_text("x\n")
            self.assertEqual(run(eng, prefix), 1)
            self.assertFalse((eng / "lib/wine/x86_64-windows/nvngx.dll").exists())
            self.assertFalse((eng / "lib/wine/x86_64-unix/nvngx.so").exists())
            self.assertFalse((prefix / "wine-calls").exists())

    def test_play_sh_turns_metalfx_on_only_when_the_bridge_is_ready(self):
        text = (ROOT / "scripts/play.sh").read_text()
        self.assertIn('gptk-dlss.sh" "$ENGINE" "$WINEPREFIX" && export D3DM_ENABLE_METALFX=1', text)
        self.assertIn("SOJU_DLSS", text)
        # It may start the bottle's wineserver; one started before WINEMSYNC=1 turns the launcher away.
        call = text.index("dlss_on ;; esac")
        self.assertLess(text.index("export WINEMSYNC=1"), call)
        xbox = (ROOT / "scripts/xbox-wine.sh").read_text()
        self.assertLess(xbox.index("WINEMSYNC"), xbox.index("gptk-dlss.sh"))
        self.assertIn("D3DM_ENABLE_METALFX=1", xbox)


if __name__ == "__main__":
    unittest.main()
