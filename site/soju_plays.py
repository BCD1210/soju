#!/usr/bin/env python3
"""Games Soju users have played, for the game pages (site/mac_games.py labels them "Plays").

Reads the anonymous play counts (site-worker/stats.js, the `plays` table) as JSON on stdin and
writes site/soju-plays.json with one line per game: id, title, how many installs played it and
for how many minutes in all. A game counts once it stayed open a few minutes in all. A start
that "ran" (open for a minute) is not enough: an anti-cheat or error window stays that long too. Nothing that identifies a person or a Mac is written.
"""
import json
import sys
from pathlib import Path

OUT = Path(__file__).resolve().parent / "soju-plays.json"
MIN_MINUTES = 3   # one start that stayed open a few minutes; a crash at launch never gets here
SQL = "select game, max(title) title, count(distinct install) players, sum(minutes) minutes from plays group by game"


def games(rows):
    out = []
    for r in rows:
        if not r.get("game") or int(r.get("minutes") or 0) < MIN_MINUTES:
            continue
        out.append({"game": str(r["game"])[:120], "title": str(r.get("title") or "")[:120],
                    "players": int(r.get("players") or 0), "minutes": int(r["minutes"])})
    return sorted(out, key=lambda g: (-g["players"], -g["minutes"], g["game"]))


if __name__ == "__main__":
    if sys.argv[1:] == ["--sql"]:
        print(SQL)
        sys.exit()
    text = sys.stdin.read()
    rows = json.loads(text[text.index("["):])[0]["results"]
    OUT.write_text(json.dumps({"games": games(rows)}, ensure_ascii=False, indent=1) + "\n")
    print(f"{OUT.name}: {len(games(rows))} games")
