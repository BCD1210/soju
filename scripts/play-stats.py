#!/usr/bin/env python3
"""Anonymous play counts for the game pages at didimlabs.com.

  soju stats play <library id> --launcher L --title T --path P [--pid N]
      Waits for the game to start, counts the minutes it stays open, then sends the
      game, its store and the minutes, with the Mac model, chip, macOS and Soju version.
      Also sends how the start went: ran, quick_exit (closed within a minute) or no_start.
  soju stats watch
      While Steam runs, counts games started from Steam's own window the same way.
  soju stats library
      Which games are installed (library id, title, store), only when that list changed.
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
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BASE = Path(os.environ.get("SOJU_BASE", str(Path.home() / ".battlenet-macos")))
URL = os.environ.get("SOJU_STATS_URL", "https://didimlabs.com/api/stats")
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
    return track_run(found, pid, clock, sleep, poll)[1]


def outcome(started, minutes):
    """How a start went: the game never appeared, closed within about a minute, or ran."""
    if not started:
        return "no_start"
    return "ran" if minutes >= 1 else "quick_exit"


def track_run(found, pid=None, clock=time.monotonic, sleep=time.sleep, poll=POLL):
    """(started, minutes): whether the game appeared at all, and how long it stayed open."""
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
                return False, 0
        else:
            misses += 1
            if misses >= GONE_AFTER:
                break
        if first is not None and now - first > MAX_HOURS * 3600:
            break
    return True, int((last - first + poll) // 60)


def report(game, title, launcher, started, minutes, how):
    """One start: its outcome always, and the minutes when there were any. Nothing once sharing is off."""
    if not enabled():
        return
    item = {"game": game[:120], "title": title[:120], "launcher": launcher[:20]}
    send({"kind": "launch", "games": [dict(item, result=outcome(started, minutes), how=how)]})
    if minutes >= 1:
        send({"kind": "play", "games": [dict(item, minutes=min(minutes, MAX_HOURS * 60))]})


STEAM_PROCESS = "\\steam\\steam.exe"
STEAM_GRACE = 5 * 60              # Steam may take this long to show its window


def steam_games(snapshot):
    """Installed Steam games with the strings their processes carry."""
    out = {}
    for g in snapshot.get("games", []):
        if g.get("platform") != "steam" or not installed(g):
            continue
        found = needles(g.get("install_path"))
        if found:
            out[g["id"]] = (g.get("title", ""), found)
    return out


def load_snapshot():
    """Steam games as installed now: read from Steam's own records, so a game installed after the
    library's last refresh is counted too. Falls back to that refresh's list."""
    try:
        import importlib.util
        sys.path.insert(0, str(ROOT / "scripts"))
        spec = importlib.util.spec_from_file_location("soju_game_library", ROOT / "scripts/game-library.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        lib = module.Library(BASE)
        lib.steam()
        return {"games": list(lib.games.values())}
    except Exception:
        pass
    try:
        return json.loads((BASE / "logs/library-scan.log").read_text())
    except (OSError, ValueError):
        return {}


def watch(clock=time.monotonic, sleep=time.sleep, poll=POLL, table=None, snapshot=load_snapshot, deliver=report):
    """While Steam runs, count the games started from Steam's own window. A game that the
    library's Play button is already counting (its lock is held) is left to that count.
    Turning sharing off stops the watch at once and drops what it was counting."""
    table = table or (lambda: run("ps", "-axww", "-o", "command=").lower())
    begun = clock()
    games, seen, missing, polls = {}, {}, 0, 0
    try:
        while True:
            if polls % 15 == 0:
                games = steam_games(snapshot())
            polls += 1
            sleep(poll)
            now = clock()
            if not enabled():
                for gid in list(seen):
                    seen.pop(gid)["lock"].unlink(missing_ok=True)
                return
            lines = table().splitlines()
            missing = 0 if any(STEAM_PROCESS in line for line in lines) else missing + 1
            for gid, (title, found) in games.items():
                up = any(n in line for line in lines for n in found)
                st = seen.get(gid)
                if up and st is None:
                    lock = claim(gid)
                    if lock is not None:
                        seen[gid] = {"first": now, "last": now, "misses": 0, "lock": lock, "title": title}
                elif up:
                    st["last"], st["misses"] = now, 0
                elif st is not None:
                    st["misses"] += 1
                    if st["misses"] >= GONE_AFTER:
                        finish(gid, seen.pop(gid), poll, deliver)
            if missing >= GONE_AFTER and now - begun > STEAM_GRACE:
                break
            if now - begun > 24 * 3600:
                break
    finally:
        for gid in list(seen):
            finish(gid, seen.pop(gid), poll, deliver)


def finish(gid, st, poll, deliver):
    st["lock"].unlink(missing_ok=True)
    deliver(gid, st["title"], "steam", True, int((st["last"] - st["first"] + poll) // 60), "steam")


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


def library_digest(payload):
    import hashlib
    ids = sorted(g["game"] for g in payload["games"])
    return hashlib.sha256("\n".join(ids).encode()).hexdigest()


def claim(game):
    """A lock file per game, so starting a game twice does not count its minutes twice."""
    folder = BASE / "stats-running"
    folder.mkdir(parents=True, exist_ok=True)
    path = folder / ("".join(c if c.isalnum() or c in "-_" else "-" for c in game)[:100] + ".pid")
    for _ in range(2):
        try:
            fd = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
            os.write(fd, str(os.getpid()).encode())
            os.close(fd)
            return path
        except FileExistsError:
            try:
                os.kill(int(path.read_text().strip() or 0), 0)
                return None            # another count of this game is running
            except (OSError, ValueError):
                path.unlink(missing_ok=True)   # left by a count that ended without cleaning up
    return None


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
    sub.add_parser("watch")
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
        try:
            snapshot = json.loads((BASE / "logs/library-scan.log").read_text())
        except (OSError, ValueError):
            return 0
        payload = library_payload(snapshot)
        # Sent only when the list of installed games changed since the last time it went through.
        stamp = BASE / "stats-library-sent"
        digest = library_digest(payload)
        if not payload["games"] or (stamp.exists() and stamp.read_text().strip() == digest):
            return 0
        if send(payload):
            stamp.write_text(digest + "\n")
        return 0
    if a.cmd == "watch":
        lock = claim("steam-watch")
        if lock is None:
            return 0   # Steam is already being watched
        try:
            watch()
        finally:
            lock.unlink(missing_ok=True)
        return 0
    found = needles(a.path)
    pid = a.pid if a.launcher in RUNS_IN_SOJU else None
    if not found and not pid:
        return 0
    lock = claim(a.game)
    if lock is None:
        return 0   # this game is already being counted
    try:
        started, minutes = track_run(found, pid)
    finally:
        lock.unlink(missing_ok=True)
    report(a.game, a.title, a.launcher, started, minutes, "soju")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
