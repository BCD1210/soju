"""Which anti-cheat a game folder carries, by the names those leave in it.

Used by the library (a note on the game: these usually close right after starting on a Mac)
and by the play counts (scripts/play-stats.py), which send the short name with a start that did not run.
"""
import json
import os
import re
import sys
from pathlib import Path

DENIED = Path(__file__).resolve().parent.parent / "resources" / "anticheat-denied.json"

ANTICHEAT = (("eac", "Easy Anti-Cheat", ("easyanticheat", "easyanticheat_eos", "start_protected_game.exe")),
             ("battleye", "BattlEye", ("battleye", "beservice.exe", "beservice_x64.exe")),
             ("vanguard", "Riot Vanguard", ("vgk.sys", "vgc.exe")),
             ("gameguard", "nProtect GameGuard", ("gameguard", "gameguard.des")),
             ("xigncode", "XIGNCODE3", ("xigncode",)),
             ("punkbuster", "PunkBuster", ("pb", "punkbuster")))


def anticheat(path):
    """Its short name ("eac", "battleye", ...) or "": the game folder, the folders right under it, and the
    <project>/Binaries/Win64 folder of an Unreal game, where those keep it (Fortnite's EasyAntiCheat is
    Fortnite/FortniteGame/Binaries/Win64/EasyAntiCheat), are looked at."""
    root = Path(str(path or ""))
    if len(str(root)) < 8 or not root.is_dir():
        return ""
    names = set()

    def add(folder):
        try:
            names.update(e.name.lower() for e in os.scandir(folder))
        except OSError:
            pass
    try:
        for entry in os.scandir(root):
            names.add(entry.name.lower())
            if entry.is_dir(follow_symlinks=False) and len(names) < 4000:
                add(entry.path)
                add(os.path.join(entry.path, "Binaries", "Win64"))
    except OSError:
        return ""
    for name, _, marks in ANTICHEAT:
        if any(m in names for m in marks):
            return name
    return ""


def note(path):
    """What the library says about such a game, or None."""
    found = anticheat(path)
    if not found:
        return None
    label = next(l for n, l, _ in ANTICHEAT if n == found)
    return ("This game uses " + label + ". Anti-cheat of this kind looks for Windows itself, so the game may close "
            "right after it starts, or play offline only.")


# Games missing from Are We Anti-Cheat Yet? (or listed as working on Linux only) that never start on a Mac.
# Easy Anti-Cheat's Wine support loads a Linux library, so "works on Proton" does not mean "works on a Mac"
# (CodeWeavers, 2026-08-31: why most games with anti-cheat do not work with CrossOver Mac).
EXTRA = [
    {"title": "Grand Theft Auto V Enhanced", "appid": None, "anticheat": "BattlEye"},
    {"title": "Grand Theft Auto V Legacy", "appid": None, "anticheat": "BattlEye"},
    {"title": "Rocket League", "appid": "252950", "anticheat": "Easy Anti-Cheat"},
    # Tested on a Mac in Soju, 2026-11-25: Easy Anti-Cheat says it has no module for this platform.
    {"title": "ROB RUN", "appid": "4262460", "anticheat": "Easy Anti-Cheat"},
    # Tested the same day: closes two seconds after start, before Unity loads (Anti-Cheat Expert, per PCGamingWiki).
    {"title": "ヘブンバーンズレッド", "appid": "1973710", "anticheat": "Anti-Cheat Expert"},
]


def _key(title):
    return re.sub(r"[^a-z0-9]+", "", re.sub(r"[™®©]", "", str(title or "").lower()))


def denied(title, appid=None, path=None, table=None):
    """The anti-cheat that stops this game outside Windows by its makers' choice, or "": games listed as
    Denied on Are We Anti-Cheat Yet? (resources/anticheat-denied.json), and any game carrying Riot Vanguard,
    a kernel driver Wine cannot load. Such a game never starts on a Mac, so Play says so first."""
    try:
        games = table if table is not None else json.loads(DENIED.read_text())["games"]
    except (OSError, ValueError, KeyError):
        games = []
    key = _key(title)
    for g in games:
        if (appid and g.get("appid") == str(appid)) or (key and _key(g.get("title")) == key):
            return g.get("anticheat") or "its anti-cheat"
    if path and anticheat(path) == "vanguard":
        return "Riot Vanguard"
    return ""


def refresh(source):
    """Rewrite resources/anticheat-denied.json from site/anticheat.json (scripts/ for the site keep that one current)."""
    data = json.loads(Path(source).read_text())
    out = [{"title": g["title"], "appid": g.get("appid"), "anticheat": ", ".join(g.get("anticheats") or [])}
           for g in data["games"].values() if g.get("status") == "Denied" and "(" not in g["title"]]
    out += [dict(g) for g in EXTRA]
    out.sort(key=lambda g: g["title"].lower())
    DENIED.write_text(json.dumps({"_source": "Are We Anti-Cheat Yet? (MIT), games whose makers deny Wine/Proton; "
                                  "refresh with scripts/soju_anticheat.py --refresh", "games": out},
                                 ensure_ascii=False, indent=1) + "\n")
    return len(out)


if __name__ == "__main__" and sys.argv[1:2] == ["--refresh"]:
    print(refresh(Path(__file__).resolve().parent.parent / "site" / "anticheat.json"))
