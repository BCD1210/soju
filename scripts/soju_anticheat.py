"""Which anti-cheat a game folder carries, by the names those leave in it.

Used by the library (a note on the game: these usually close right after starting on a Mac)
and by the play counts (scripts/play-stats.py), which send the short name with a start that did not run.
"""
import os
from pathlib import Path

ANTICHEAT = (("eac", "Easy Anti-Cheat", ("easyanticheat", "easyanticheat_eos", "start_protected_game.exe")),
             ("battleye", "BattlEye", ("battleye", "beservice.exe", "beservice_x64.exe")),
             ("vanguard", "Riot Vanguard", ("vgk.sys", "vgc.exe")),
             ("gameguard", "nProtect GameGuard", ("gameguard", "gameguard.des")),
             ("xigncode", "XIGNCODE3", ("xigncode",)),
             ("punkbuster", "PunkBuster", ("pb", "punkbuster")))


def anticheat(path):
    """Its short name ("eac", "battleye", ...) or "": the game folder and the folders right under it are looked at."""
    root = Path(str(path or ""))
    if len(str(root)) < 8 or not root.is_dir():
        return ""
    names = set()
    try:
        for entry in os.scandir(root):
            names.add(entry.name.lower())
            if entry.is_dir(follow_symlinks=False) and len(names) < 4000:
                try:
                    names.update(e.name.lower() for e in os.scandir(entry.path))
                except OSError:
                    pass
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
