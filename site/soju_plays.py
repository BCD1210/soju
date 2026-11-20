#!/usr/bin/env python3
"""Games Soju users have played, for the game pages (site/mac_games.py labels them "Plays").

Reads the anonymous play counts and start results (site-worker/stats.js, the `plays` and
`launches` tables) as JSON on stdin and writes site/soju-plays.json with one line per game: id,
title, how many installs played it and for how many minutes in all. A game counts once it stayed
open a few minutes or once a start reached the game ("ran"). Nothing that identifies a person or a Mac is written. Run it on a
Mac logged in to the account that holds the soju-stats database:

    cd site-worker && npx wrangler d1 execute soju-stats --remote --json -c wrangler.didimlabs.toml \\
      --command "$(python3 ../site/soju_plays.py --sql)" \\
      | python3 ../site/soju_plays.py
"""
import json
import sys
from pathlib import Path

OUT = Path(__file__).resolve().parent / "soju-plays.json"
MIN_MINUTES = 3   # one start that stayed open a few minutes; a crash at launch never gets here
SQL = ("select game, max(title) title, count(distinct install) players, sum(minutes) minutes, max(ran) ran from ("
       "select game, title, install, minutes, 0 ran from plays union all "
       "select game, title, install, 0, n from launches where result = 'ran') group by game")


def games(rows):
    out = []
    for r in rows:
        if not r.get("game") or (int(r.get("minutes") or 0) < MIN_MINUTES and int(r.get("ran") or 0) < 1):
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
