#!/usr/bin/env python3
"""What Battle.net did with a request to start a game, read from Battle.net's own logs.

Battle.net writes a new battle.net-<time>.log each time it starts (in the bottle, under
users/<name>/AppData/Local/Battle.net/Logs), with lines like:
  I 2026-11-22 17:53:23.851176 [GameLaunchController] {Main} LaunchBinary: uid=osi selectedRegion=KR binaryType=game
  I 2026-11-22 17:57:17.832215 [BNLogin] {Main} Logged into Battle.net successfully. ...
  I ... [ProductState] {Main} InstallState (osi): currentOperation=Update ...
Times are UTC.

  battlenet_launch.py watch <bottle> <game uid> [--seconds N] [-- <command that hands --exec="launch X" over>]
      Runs the command, then follows the logs. A start that goes through ends quietly; one that waits for the
      player's sign-in or for an update ends with a "soju: ..." line the app shows, and a non-zero status.
      ("Pre-existing game session detected" is not a sign of trouble: Battle.net writes it right after
      every start it performs, seen 2026-11-23.)

Also used by scripts/play-stats.py: reason(bottle, uid, since) gives the word sent with a start that did not run.
"""
import glob
import os
import re
import subprocess
import sys
import time

LOGS = "drive_c/users/*/AppData/Local/Battle.net/Logs/battle.net-*.log"
STAMP = re.compile(r"^[A-Z] (\d{4}-\d\d-\d\d \d\d:\d\d:\d\d)")


def text_since(bottle, since):
    """Log lines written since `since` (epoch seconds), from the logs touched since then."""
    stamp = time.strftime("%Y-%m-%d %H:%M:%S", time.gmtime(since))
    out = []
    for path in glob.glob(os.path.join(str(bottle), LOGS)):
        try:
            if os.path.getmtime(path) < since - 5:
                continue
            with open(path, "r", encoding="utf-8", errors="replace") as f:
                for line in f:
                    m = STAMP.match(line)
                    if m and m.group(1) >= stamp:
                        out.append(line)
        except OSError:
            pass
    return "".join(out)


def signed_in(bottle):
    """Whether the Battle.net running now (its newest log) says it signed in."""
    logs = glob.glob(os.path.join(str(bottle), LOGS))
    if not logs:
        return False
    try:
        newest = max(logs, key=os.path.getmtime)
        with open(newest, "r", encoding="utf-8", errors="replace") as f:
            return "Logged into Battle.net successfully" in f.read()
    except OSError:
        return False


def outcome(text, uid):
    """"ok" (Battle.net started the game or it is running), "updating", or ""."""
    uid = re.escape(uid.lower())
    if re.search(r"LaunchBinary: uid=%s\b" % uid, text) or re.search(r"Game is running: %s\b" % uid, text):
        return "ok"
    if re.search(r"InstallState \(%s\): currentOperation=Update\b" % uid, text) or re.search(r"Update started: %s\b" % uid, text):
        return "updating"
    return ""


def reason(bottle, uid, since):
    """The word for a start that did not run: bnet_launched when Battle.net did start it (so the game closed
    or was not seen), bnet_updating, bnet_signin, or ""."""
    kind = outcome(text_since(bottle, since), uid)
    if kind == "ok":
        return "bnet_launched"
    if kind:
        return "bnet_" + kind
    return "" if signed_in(bottle) else "bnet_signin"


MESSAGES = {
    "updating": "Battle.net is updating the game first. It starts by itself when the update in Battle.net finishes.",
    "signin": "Battle.net is waiting for you to sign in. Its window is open now: sign in there and the game starts by itself.",
}


def watch(bottle, uid, command, seconds=60, waiting_after=15, signin_after=60, show=None,
          poll=1.0, clock=time.monotonic, sleep=time.sleep, now=time.time):
    since = now() - 2
    code = subprocess.call(command) if command else 0
    if code != 0:
        return code
    begun = clock()
    first = {}
    while clock() - begun < seconds:
        sleep(poll)
        kind = outcome(text_since(bottle, since), uid)
        if kind == "ok":
            return 0
        if kind:
            first.setdefault(kind, clock())
            if clock() - first[kind] >= waiting_after:
                print("soju: " + MESSAGES[kind], flush=True)
                return 4
        elif clock() - begun >= signin_after and not signed_in(bottle):
            if show:
                try:
                    subprocess.call(show, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=30)
                except (OSError, subprocess.SubprocessError):
                    pass
            print("soju: " + MESSAGES["signin"], flush=True)
            return 4
    return 0


def main(argv):
    if len(argv) >= 3 and argv[0] == "watch":
        command = argv[argv.index("--") + 1:] if "--" in argv else []
        head = argv[:argv.index("--")] if "--" in argv else argv
        seconds = int(head[head.index("--seconds") + 1]) if "--seconds" in head else 60
        show = os.environ.get("SOJU_BNET_SHOW")
        return watch(argv[1], argv[2], command, seconds=seconds, show=show.split("\x1f") if show else None)
    print("usage: battlenet_launch.py watch <bottle> <uid> [--seconds N] [-- <command>]", file=sys.stderr)
    return 64


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
