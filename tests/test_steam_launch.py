import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
import steam_launch as sl  # noqa: E402

OK = """[t] GameAction [AppID 70, ActionID 1] : LaunchApp changed task to ShowInterstitials with ""
[t] GameAction [AppID 70, ActionID 1] : LaunchApp waiting for user response to ShowInterstitials ""
[t] GameAction [AppID 70, ActionID 1] : LaunchApp continues with user response "ShowInterstitials"
[t] GameAction [AppID 70, ActionID 1] : LaunchApp changed task to CreatingProcess with ""
[t] GameAction [AppID 70, ActionID 1] : LaunchApp changed task to Completed with ""
"""
FAIL = """[t] GameAction [AppID 70, ActionID 2] : LaunchApp changed task to UpdatingAppTicket with ""
[t] GameAction [AppID 70, ActionID 2] : LaunchApp failed with AppError_18 with ""
[t] GameAction [AppID 70, ActionID 2] : LaunchApp changed task to Failed with ""
"""
ASK = '[t] GameAction [AppID 70, ActionID 3] : LaunchApp waiting for user response to ShowEula ""\n'


def fake_time():
    t = [0.0]
    return (lambda: t[0]), (lambda s: t.__setitem__(0, t[0] + s))


class SteamLaunchTests(unittest.TestCase):
    def test_reads_how_a_start_went(self):
        self.assertEqual(sl.outcome(OK, "70"), ("ok", ""))
        self.assertEqual(sl.outcome(FAIL, "70"), ("error", "AppError_18"))
        self.assertEqual(sl.outcome(ASK, "70"), ("waiting", "ShowEula"))
        self.assertEqual(sl.outcome(OK + FAIL, "71"), ("", ""))   # another game's start says nothing
        self.assertIn("not installed", sl.message("error", "AppError_18"))

    def test_watch_says_why_and_stays_quiet_when_it_works(self):
        with tempfile.TemporaryDirectory() as d:
            logs = Path(d) / "logs"
            logs.mkdir()
            log = logs / "console_log.txt"
            log.write_text("earlier lines\n" + FAIL)   # an older start of the same game is not read
            for body, code in ((OK, 0), (FAIL, 3), (ASK, 4)):
                cmd = [sys.executable, "-c", "open(%r,'a').write(%r)" % (str(log), body)]
                clock, sleep = fake_time()
                self.assertEqual(sl.watch(d, "70", cmd, clock=clock, sleep=sleep), code)
            # Nothing about the game within the minute: no message, no failure.
            clock, sleep = fake_time()
            self.assertEqual(sl.watch(d, "999", [sys.executable, "-c", "pass"], clock=clock, sleep=sleep), 0)

    def test_a_wait_brings_steams_window_forward_and_a_sign_in_is_named(self):
        with tempfile.TemporaryDirectory() as d:
            logs = Path(d) / "logs"
            logs.mkdir()
            (logs / "console_log.txt").write_text("")
            shown = Path(d) / "shown"
            show = [sys.executable, "-c", "open(%r,'w').write('x')" % str(shown)]
            cmd = [sys.executable, "-c", "open(%r,'a').write(%r)" % (str(logs / "console_log.txt"), ASK)]
            clock, sleep = fake_time()
            self.assertEqual(sl.watch(d, "70", cmd, show=show, clock=clock, sleep=sleep), 4)
            self.assertTrue(shown.exists())
            shown.unlink()
            (logs / "connection_log.txt").write_text("[t] [Logged Off, 4, 0] [U:1:1] LogOn() called\n")
            clock, sleep = fake_time()
            self.assertEqual(sl.watch(d, "71", [], show=show, seconds=150, clock=clock, sleep=sleep), 4)
            self.assertTrue(shown.exists())
            # Signed in and simply nothing yet: no message.
            (logs / "connection_log.txt").write_text("[t] [Logged On, 4, 7] [U:1:1] RecvMsgClientLogOnResponse() : processing complete\n")
            clock, sleep = fake_time()
            self.assertEqual(sl.watch(d, "72", [], seconds=150, clock=clock, sleep=sleep), 0)

    def test_play_sh_follows_the_start_only_when_steam_is_open(self):
        text = (ROOT / "scripts/play.sh").read_text()
        self.assertIn('steam_launch.py" watch', text)
        self.assertEqual(text.count('${STEAM_WATCH[@]+"${STEAM_WATCH[@]}"}'), 2)
        self.assertEqual(text.count('exec "${STEAM_WATCH[@]}"'), 2)   # a cold start, in both Steam modes


if __name__ == "__main__":
    unittest.main()
