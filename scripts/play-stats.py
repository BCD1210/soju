#!/usr/bin/env python3
"""Anonymous play counts for the game pages at didimlabs.com.

  soju stats play <library id> --launcher L --title T --path P [--pid N]
      Waits for the game to start, counts the minutes it stays open, then sends the
      game, its store and the minutes, with the Mac model, chip, macOS and Soju version.
  soju stats library
      At most once a day: which games are installed (library id, title, store).
  soju stats on | off | status

Nothing else leaves the Mac: no account, email, user name or file path. A random id
(stats-id in the Soju folder) lets the site count players rather than sessions.
Turn it off in Soju (Platforms > Share which games I play), with `soju stats off`,
or with SOJU_NO_STATS=1.
"""
import argparse
import json
import os
import platform
import subprocess
import sys
import time
import urllib.request
import uuid
from datetime import date
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BASE = Path(os.environ.get("SOJU_BASE", str(Path.home() / ".battlenet-macos")))
URL = os.environ.get("SOJU_STATS_URL", "https://soju-stats.mikey9220.workers.dev/api/stats")
OFF = BASE / "stats-off"
QUEUE = BASE / "stats-queue.jsonl"
POLL = 20                # seconds between process checks
START_WITHIN = 15 * 60   # a game that has not started by then is not counted
GONE_AFTER = 3           # checks in a row without the game: it has quit
MAX_HOURS = 12
RUNS_IN_SOJU = ("xbox", "didim")   # the soju game process is the game itself


def enabled():
    return not OFF.exists() and os.environ.get("SOJU_NO_STATS") != "1"


def install_id():
    path = BASE / "stats-id"
    try:
        value = path.read_text().strip()
        uuid.UUID(value)
        return value
    except (OSError, ValueError):
        value = str(uuid.uuid4())
        BASE.mkdir(parents=True, exist_ok=True)
        path.write_text(value + "\n")
        os.chmod(path, 0o600)
        return value


def run(*cmd):
    try:
        return subprocess.run(cmd, capture_output=True, text=True, timeout=10).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return ""


def machine():
    try:
        version = (ROOT / "VERSION").read_text().strip()
    except OSError:
        version = ""
    return {"mac": run("sysctl", "-n", "hw.model")[:40], "chip": run("sysctl", "-n", "machdep.cpu.brand_string")[:60],
            "macos": platform.mac_ver()[0][:20], "soju": version[:20]}


def needles(path):
    """Strings that appear in the command line of a process started from the game folder."""
    s = str(path or "").rstrip("/")
    if len(s) < 8:
        return []
    out = [s.lower() + "/"]
    i = s.lower().find("/drive_c/")
    if i >= 0 and len(s) - i > 12:
        out.append("c:\\" + s[i + 9:].replace("/", "\\").lower() + "\\")
    return out


def running(found, pid=None):
    if pid:
        try:
            os.kill(pid, 0)
            return True
        except OSError:
            return False
    table = run("ps", "-axww", "-o", "command=").lower()
    return any(n in line for line in table.splitlines() for n in found)


def track(found, pid=None, clock=time.monotonic, sleep=time.sleep, poll=POLL):
    """Minutes the game was open, 0 if it never started."""
    begun = clock()
    first = last = None
    misses = 0
    while True:
        sleep(poll)
        now = clock()
        if running(found, pid):
            first = first or now
            last = now
            misses = 0
        elif first is None:
            if now - begun > START_WITHIN:
                return 0
        else:
            misses += 1
            if misses >= GONE_AFTER:
                break
        if first is not None and now - first > MAX_HOURS * 3600:
            break
    return int((last - first + poll) // 60)


def post(body):
    req = urllib.request.Request(URL, data=json.dumps(body).encode(), method="POST",
                                 headers={"content-type": "application/json", "user-agent": "soju-stats"})
    with urllib.request.urlopen(req, timeout=15) as res:
        return 200 <= res.status < 300


def send(payload):
    body = {"id": install_id(), **machine(), **payload}
    pending = []
    try:
        pending = [json.loads(line) for line in QUEUE.read_text().splitlines() if line.strip()]
    except (OSError, ValueError):
        pass
    left = []
    for item in pending + [body]:
        try:
            if not post(item):
                left.append(item)
        except Exception:
            left.append(item)
    try:
        if left:
            QUEUE.write_text("".join(json.dumps(i) + "\n" for i in left[-50:]))
            os.chmod(QUEUE, 0o600)
        elif QUEUE.exists():
            QUEUE.unlink()
    except OSError:
        pass
    return not left


def installed(game):
    if game.get("platform") == "xbox":
        return game.get("downloaded") is True
    return bool(game.get("install_path"))


def library_payload(snapshot):
    games = [{"game": g["id"][:120], "title": (g.get("title") or "")[:120], "launcher": g.get("platform", "")[:20]}
             for g in snapshot.get("games", []) if g.get("id") and installed(g)]
    return {"kind": "library", "games": games[:500]}


def main(argv):
    ap = argparse.ArgumentParser(prog="soju stats")
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("play")
    p.add_argument("game")
    p.add_argument("--launcher", required=True)
    p.add_argument("--title", default="")
    p.add_argument("--path", default="")
    p.add_argument("--pid", type=int)
    sub.add_parser("library")
    for name in ("on", "off", "status"):
        sub.add_parser(name)
    a = ap.parse_args(argv)

    if a.cmd == "off":
        BASE.mkdir(parents=True, exist_ok=True)
        OFF.touch()
        print("Sharing which games you play is off.")
        return 0
    if a.cmd == "on":
        OFF.unlink(missing_ok=True)
        print("Sharing which games you play is on.")
        return 0
    if a.cmd == "status":
        print("on" if enabled() else "off")
        return 0
    if not enabled():
        return 0
    if a.cmd == "library":
        stamp = BASE / "stats-library-day"
        today = date.today().isoformat()
        if stamp.exists() and stamp.read_text().strip() == today:
            return 0
        try:
            snapshot = json.loads((BASE / "logs/library-scan.log").read_text())
        except (OSError, ValueError):
            return 0
        payload = library_payload(snapshot)
        if payload["games"] and send(payload):
            stamp.write_text(today + "\n")
        return 0
    found = needles(a.path)
    pid = a.pid if a.launcher in RUNS_IN_SOJU else None
    if not found and not pid:
        return 0
    minutes = track(found, pid)
    if minutes >= 1:
        send({"kind": "play", "games": [{"game": a.game[:120], "title": a.title[:120], "launcher": a.launcher[:20],
                                         "minutes": min(minutes, MAX_HOURS * 60)}]})
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
