import importlib.util
from pathlib import Path
import struct
import unittest
from unittest.mock import patch
from types import SimpleNamespace

from make_duel_fixture import scenario
from make_fixture import sections, unit

spec = importlib.util.spec_from_file_location("native_launcher", Path(__file__).resolve().parents[2]/"scripts/broodwar-native.py")
launcher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(launcher)


class DuelTests(unittest.TestCase):
    def source(self):
        return [(b"DIM ", struct.pack("<HH", 64, 64)), (b"MTXM", b"owned terrain"),
                (b"UNIT", unit(1, 480, 368, 214)+unit(2, 1568, 1680, 214, 1)+unit(3, 400, 400, 176, 11, 1500)),
                (b"OWNR", bytes(12)), (b"IOWN", bytes(12)), (b"SIDE", bytes(12)),
                (b"FORC", bytes(20)), (b"TRIG", bytes(2400)), (b"MBRF", bytes(2400)), (b"MRGN", bytes(5100))]

    def test_duel_preserves_terrain_and_has_two_races(self):
        packed, starts = scenario(self.source())
        data = dict(sections(packed))
        self.assertEqual(data[b"MTXM"], b"owned terrain")
        self.assertEqual(data[b"OWNR"], bytes([6, 5]+[0]*10))
        self.assertEqual(data[b"SIDE"][:2], bytes([1, 0]))
        self.assertEqual(len(starts), 2)
        units = [data[b"UNIT"][i:i+36] for i in range(0, len(data[b"UNIT"]), 36)]
        self.assertEqual(sum(u[16] == 0 for u in units), 6)
        self.assertEqual(sum(u[16] == 1 for u in units), 7)
        self.assertEqual(data[b"TRIG"][2400+352+20:2400+352+24], b"ZMCx")
        self.assertEqual(data[b"TRIG"][2400+2373], 1)

    def test_rejects_non_duel_source(self):
        source = self.source()
        source[2] = (b"UNIT", unit(1, 480, 368, 214))
        with self.assertRaisesRegex(ValueError, "two-start"):
            scenario(source)

    def test_process_scope_excludes_other_bottles_and_shells(self):
        game = launcher.windows(launcher.GAME/"x86_64/StarCraft.exe")
        loader = str(launcher.RUNTIME/"samase64.exe")
        listing = f"101 {game} -launch\n102 {launcher.ENGINE}/lib/wine/x86_64-unix/wine {loader}\n103 /bin/zsh -c {loader}\n104 Z:\\other\\StarCraft.exe -launch\n105 /other/wine {loader}\n106 /usr/bin/python devplay.py\n"
        with patch.object(launcher.subprocess, "run", return_value=SimpleNamespace(stdout=listing)):
            self.assertEqual(launcher.dedicated_processes(), [101, 102])

    def test_stop_does_not_signal_unrelated_processes(self):
        with patch.object(launcher, "check_prefix"), patch.object(launcher.subprocess, "run"), \
             patch.object(launcher, "dedicated_processes", return_value=[]), patch.object(launcher.os, "kill") as kill:
            launcher.stop()
            kill.assert_not_called()


if __name__ == "__main__":
    unittest.main()
