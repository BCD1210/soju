"""soju ue: a packaged Unreal Engine Windows build on the CX engine, with a fake engine that records its calls."""
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts/ue-wine.sh"


def fake_engine(base):
    eng = base / "cx26-engine"
    (eng / "bin").mkdir(parents=True)
    (eng / "lib/external").mkdir(parents=True)
    (eng / "lib/external/libd3dshared.dylib").write_bytes(b"dylib")
    wine = eng / "bin/wine"   # records where and how it was started instead of running Windows code
    wine.write_text('#!/bin/sh\n[ "$1" = wineboot ] && { : > "$WINEPREFIX/system.reg"; exit 0; }\n'
                    'echo "$PWD|$WINEPREFIX|$CX_GRAPHICS_BACKEND|$*" >> "$SOJU_BASE/wine-calls"\n')
    server = eng / "bin/wineserver"
    server.write_text('#!/bin/sh\necho "$WINEPREFIX $*" >> "$SOJU_BASE/wineserver-calls"\n')
    for f in (wine, server):
        f.chmod(0o755)


def package(folder, project):
    win64 = folder / project / "Binaries/Win64"
    win64.mkdir(parents=True)
    (win64 / (project + "-Win64-Shipping.exe")).write_bytes(b"MZ")
    (folder / (project + ".exe")).write_bytes(b"MZ")
    (folder / "Engine/Binaries/Win64").mkdir(parents=True)
    (folder / "Engine/Binaries/Win64/CrashReportClient.exe").write_bytes(b"MZ")


def ready_bottle(base, name):
    bottle = base / "ue-bottles" / name
    bottle.mkdir(parents=True)
    (bottle / "system.reg").write_text("WINE REGISTRY Version 2\n")
    (bottle / ".soju-vcrun").write_text("14\n")
    return bottle


class UnrealWineTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.base = Path(self.tmp.name).resolve() / "soju"
        fake_engine(self.base)
        env = {k: v for k, v in os.environ.items()
               if not k.startswith(("WINE", "DYLD_", "CX_", "ROSETTA_", "MTL_")) and k != "ENGINE"}
        self.env = dict(env, SOJU_BASE=str(self.base))

    def tearDown(self):
        self.tmp.cleanup()

    def run_ue(self, *args):
        return subprocess.run(["bash", str(SCRIPT), *map(str, args)], env=self.env, capture_output=True, text=True)

    def calls(self):
        return (self.base / "wine-calls").read_text().splitlines()

    def test_shipping_exe_starts_directly_in_its_own_bottle_windowed_unless_given_arguments(self):
        game = Path(self.tmp.name).resolve() / "Windows"
        package(game, "MyProject")
        bottle = ready_bottle(self.base, "MyProject")
        self.assertEqual(self.run_ue(game).returncode, 0)
        self.assertEqual(self.run_ue(game, "-log", "-ResX=1920").returncode, 0)
        first, second = self.calls()
        win64 = str(game / "MyProject/Binaries/Win64")
        self.assertEqual(first, f"{win64}|{bottle}|d3dmetal|MyProject-Win64-Shipping.exe -ResX=1280 -ResY=720 -windowed")
        self.assertEqual(second, f"{win64}|{bottle}|d3dmetal|MyProject-Win64-Shipping.exe -log -ResX=1920")
        # A ready bottle is not booted again, and the launch waits for the bottle to empty.
        self.assertIn(f"{bottle} -w", (self.base / "wineserver-calls").read_text())

    def test_package_without_a_shipping_exe_uses_the_exe_at_the_top(self):
        game = Path(self.tmp.name).resolve() / "Dev"
        game.mkdir()
        (game / "Game.exe").write_bytes(b"MZ")
        bottle = ready_bottle(self.base, "Game")
        self.assertEqual(self.run_ue(game).returncode, 0)
        self.assertEqual(self.calls(), [f"{game}|{bottle}|d3dmetal|Game.exe -ResX=1280 -ResY=720 -windowed"])
        empty = Path(self.tmp.name) / "empty"
        empty.mkdir()
        result = self.run_ue(empty)
        self.assertEqual(result.returncode, 1)
        self.assertIn("no <Project>-Win64-Shipping.exe", result.stderr)

    def test_new_bottle_gets_the_pinned_vc_runtime_and_a_bad_download_stops_the_launch(self):
        game = Path(self.tmp.name).resolve() / "Windows"
        package(game, "MyProject")
        tools = Path(self.tmp.name) / "tools"
        tools.mkdir()
        curl = tools / "curl"   # a download that is not Microsoft's file
        curl.write_text('#!/bin/sh\necho "$@" >> "$SOJU_BASE/curl-calls"\n'
                        'while [ $# -gt 0 ]; do [ "$1" = --output ] && echo junk > "$2"; shift; done\n')
        curl.chmod(0o755)
        self.env["PATH"] = f"{tools}:{self.env['PATH']}"
        result = self.run_ue(game)
        self.assertEqual(result.returncode, 1)
        self.assertIn("did not match its checksum", result.stderr)
        bottle = self.base / "ue-bottles/MyProject"
        self.assertTrue((bottle / "system.reg").is_file())
        self.assertFalse((bottle / ".soju-vcrun").exists())
        self.assertFalse((self.base / "wine-calls").exists())
        self.assertEqual(list((self.base / "build").glob("vc_redist*")), [])
        self.assertIn("https://download.visualstudio.microsoft.com/", (self.base / "curl-calls").read_text())

    def test_cli_and_uninstall_know_it(self):
        self.assertIn('ue)             shift; exec bash "$ROOT/scripts/ue-wine.sh"', (ROOT / "scripts/soju").read_text())
        self.assertIn("xbox-bottle ue-bottles; do", (ROOT / "scripts/uninstall.sh").read_text())


if __name__ == "__main__":
    unittest.main()
