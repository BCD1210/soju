import os
import sys
import tempfile
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
import battlenet_launch as bl  # noqa: E402


def stamp(t):
    return time.strftime("%Y-%m-%d %H:%M:%S", time.gmtime(t)) + ".100000"


class BattleNetLaunchTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.bottle = Path(self.tmp.name)
        self.logs = self.bottle / "drive_c/users/crossover/AppData/Local/Battle.net/Logs"
        self.logs.mkdir(parents=True)

    def tearDown(self):
        self.tmp.cleanup()

    def log(self, name, lines):
        with open(self.logs / name, "a") as f:
            for t, text in lines:
                f.write("I %s [Main] {Main} %s\r\n" % (stamp(t), text))

    def test_reads_how_a_start_went_and_only_since_it_began(self):
        t = time.time()
        self.log("battle.net-1.log", [(t - 600, "LaunchBinary: uid=osi selectedRegion=KR binaryType=game"),
                                      (t - 590, "Logged into Battle.net successfully. |bnet=1")])
        self.assertEqual(bl.outcome(bl.text_since(self.bottle, t - 10), "OSI"), "")      # the old start is not this one
        self.assertEqual(bl.reason(self.bottle, "osi", t - 10), "")                     # signed in, nothing more known
        self.log("battle.net-1.log", [(t, "InstallState (osi): currentOperation=Update currentOperationStatus=New")])
        self.assertEqual(bl.reason(self.bottle, "osi", t - 10), "bnet_updating")
        self.log("battle.net-1.log", [(t + 1, "LaunchBinary: uid=osi selectedRegion=KR binaryType=game")])
        self.assertEqual(bl.reason(self.bottle, "osi", t - 10), "bnet_launched")
        # Written right after every start Battle.net performs: not a sign of trouble.
        self.assertEqual(bl.outcome("Pre-existing game session detected without a pending launch.", "osi"), "")

    def test_a_battle_net_that_never_signed_in_is_named(self):
        t = time.time()
        self.log("battle.net-2.log", [(t, "Command line arguments: {[0]=--exec=launch OSI}")])
        self.assertEqual(bl.reason(self.bottle, "osi", t - 10), "bnet_signin")

    def test_watch_is_quiet_when_the_game_starts_and_says_why_when_not(self):
        clockv = [0.0]
        def clock(): return clockv[0]
        def sleep(s): clockv[0] += s
        t = time.time()
        cmd = [sys.executable, "-c", "import time;open(%r,'a').write('I %s [x] {Main} LaunchBinary: uid=osi x\\r\\n')"
               % (str(self.logs / "battle.net-3.log"), stamp(t + 1))]
        self.assertEqual(bl.watch(self.bottle, "OSI", cmd, clock=clock, sleep=sleep), 0)
        clockv[0] = 0.0
        self.log("battle.net-4.log", [(t + 2, "Logged into Battle.net successfully.")])
        cmd = [sys.executable, "-c", "open(%r,'a').write('I %s [x] {Main} InstallState (s1): currentOperation=Update currentOperationStatus=New\\r\\n')"
               % (str(self.logs / "battle.net-4.log"), stamp(t + 3))]
        self.assertEqual(bl.watch(self.bottle, "s1", cmd, clock=clock, sleep=sleep), 4)
        # Signed in, nothing about the game for the whole minute: no message.
        clockv[0] = 0.0
        self.assertEqual(bl.watch(self.bottle, "fenris", [], clock=clock, sleep=sleep, now=lambda: t + 10), 0)

    def test_play_sh_follows_battle_net_starts(self):
        text = (ROOT / "scripts/play.sh").read_text()
        self.assertIn('battlenet_launch.py" watch', text)
        self.assertIn('exec "${BN_WATCH[@]}" --seconds 150', text)


if __name__ == "__main__":
    unittest.main()
