#!/usr/bin/env python3
"""What Steam did with a request to start a game, read from Steam's own console_log.txt.

Steam writes each step of a start there, per game:
  GameAction [AppID 1385950, ActionID 2] : LaunchApp changed task to CreatingProcess with ""
  GameAction [AppID 1385950, ActionID 2] : LaunchApp failed with AppError_18 with ""
  GameAction [AppID 1385950, ActionID 2] : LaunchApp waiting for user response to ShowEula ""

  steam_launch.py watch <Steam folder> <appid> -- <command that hands steam://rungameid/<appid> to Steam>
      Runs the command, then follows the log for up to a minute. A start that goes through ends quietly;
      one that Steam refused, or that waits for an answer in Steam's window, ends with a "soju: ..." line
      the app shows, and a non-zero status.

Also used by scripts/play-stats.py: outcome(text, appid) gives the reason sent with a start that did not run.
"""
import os
import re
import subprocess
import sys
import time

# Steps Steam passes through by itself; anything else it waits on is a question for the player.
AUTO = {"ShowInterstitials", "CreatingProcess"}
# Seen here: AppError_18 for a game in the library that is not installed in this Steam (2026-11-23).
HINTS = {
    "18": "Steam gives this when the game is not installed or not finished in this Steam. Open Steam, check the game in your Library, then press Play again.",
}


def outcome(text, appid):
    """(kind, detail) for the last start of this game in the text: ("ok", ""), ("error", "AppError_18"),
    ("waiting", "<step>"), ("updating", "<step>"), ("busy", "<step>") or ("", "") when the start is not there."""
    tag = "[AppID %s," % appid
    kind, detail = "", ""
    for line in text.splitlines():
        if tag not in line or "LaunchApp" not in line:
            continue
        if "changed task to Completed" in line:
            kind, detail = "ok", ""
        elif (m := re.search(r"failed with (AppError_\d+)", line)):
            kind, detail = "error", m.group(1)
        elif kind == "error" and "changed task to Failed" in line:
            pass
        elif (m := re.search(r"waiting for user response to (\w+)", line)):
            if m.group(1) not in AUTO:
                kind, detail = "waiting", m.group(1)
        elif (m := re.search(r"continues with user response \"?(\w+)", line)):
            if kind == "waiting" and detail == m.group(1):
                kind, detail = "busy", detail
        elif (m := re.search(r"changed task to (\w+)", line)):
            step = m.group(1)
            kind, detail = ("updating" if "Update" in step and step != "UpdatingAppTicket" else "busy"), step
    return kind, detail


def message(kind, detail):
    if kind == "error":
        number = detail.split("_")[-1]
        return "Steam could not start the game (" + detail + "). " + HINTS.get(number, "Open Steam to see why, then press Play again.")
    if kind == "waiting":
        return "Steam is waiting for you before it starts the game (" + detail + "). Click Steam in the Dock to see its window and answer; the game starts after that."
    if kind == "updating":
        return "Steam is updating the game first. It starts by itself when the download in Steam finishes."
    return ""


def watch(steam_dir, appid, command, seconds=60, waiting_after=15, poll=1.0, clock=time.monotonic, sleep=time.sleep):
    log = os.path.join(steam_dir, "logs", "console_log.txt")
    try:
        start = os.path.getsize(log)
    except OSError:
        start = 0
    code = subprocess.call(command) if command else 0
    if code != 0:
        return code
    begun = clock()
    since = None
    while clock() - begun < seconds:
        sleep(poll)
        try:
            with open(log, "rb") as f:
                f.seek(start if os.path.getsize(log) >= start else 0)
                text = f.read().decode("utf-8", "replace")
        except OSError:
            continue
        kind, detail = outcome(text, appid)
        if kind == "ok":
            return 0
        if kind == "error":
            print("soju: " + message(kind, detail), flush=True)
            return 3
        if kind in ("waiting", "updating"):
            since = since or clock()
            if clock() - since >= waiting_after:
                print("soju: " + message(kind, detail), flush=True)
                return 4
        else:
            since = None
    return 0


def main(argv):
    if len(argv) >= 3 and argv[0] == "watch":
        command = argv[argv.index("--") + 1:] if "--" in argv else []
        return watch(argv[1], argv[2], command)
    print("usage: steam_launch.py watch <Steam folder> <appid> -- <command>", file=sys.stderr)
    return 64


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
