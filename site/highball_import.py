#!/usr/bin/env python3
"""Condense Highball's open compatibility database into site/highball-mac.json.

  git clone https://github.com/gauthierpiarrette/highball-db
  python3 site/highball_import.py highball-db

db/games/*.json is CC0 (Mac runs and reports gathered by the Highball project);
db/anticheat.json derives from Are We Anti-Cheat Yet? under the MIT license, whose
notice is kept in the output. These are other people's Mac results, on Highball's
engines, and the game pages say so.
"""
import json
from pathlib import Path
import re
import subprocess
import sys

OUT = Path(__file__).resolve().parent / "highball-mac.json"


def short(text, n=420):
    text = re.sub(r"\s+", " ", text or "").strip()
    return text if len(text) <= n else text[:n].rsplit(" ", 1)[0] + "…"


def main(repo):
    repo = Path(repo)
    commit = subprocess.run(["git", "-C", str(repo), "rev-parse", "--short", "HEAD"],
                            capture_output=True, text=True).stdout.strip()
    games = {}
    for f in sorted((repo / "db/games").glob("*.json")):
        d = json.loads(f.read_text())
        status = d.get("status")
        rating = None
        m = re.search(r"rated (\d)(?: of 5)?", d.get("provenance") or "")
        if m:
            rating = int(m.group(1))
        if status == "verified-local":
            kind = "verified"
        elif status in ("blocked-anticheat", "blocked-publisher"):
            kind = "blocked"
        elif status in ("community", "reported-upstream"):
            kind = "problems" if rating is not None and rating <= 2 else "reported"
        else:
            continue
        date = re.search(r"\d{4}-\d{2}-\d{2}", d.get("provenance") or "")
        chip = re.search(r"Apple (M\d(?: (?:Pro|Max|Ultra))?)", d.get("provenance") or "")
        games[d["id"]] = {
            "title": d.get("title"), "appid": str(d["steam_appid"]) if d.get("steam_appid") else None,
            "kind": kind, "rating": rating, "renderer": d.get("renderer"),
            "date": date.group(0) if date else None, "chip": chip.group(1) if chip else None,
            "notes": short(d.get("notes")),
            "url": "https://gethighball.com/database/",
        }
    ac = json.loads((repo / "db/anticheat.json").read_text())
    anticheat = {}
    for key, g in ac["games"].items():
        if g.get("macVerdict") != "blocked":
            continue
        anticheat[key] = {"title": g.get("title"), "appid": str(g["steam_appid"]) if g.get("steam_appid") else None,
                          "anticheats": g.get("anticheats") or []}
    notice = (repo / "db/anticheat.LICENSE").read_text()
    OUT.write_text(json.dumps({
        "_source": f"https://github.com/gauthierpiarrette/highball-db at {commit}",
        "_license": "games: CC0 1.0 (Highball project). anticheat: derived from Are We Anti-Cheat Yet? (MIT), notice below.",
        "_anticheat_notice": notice,
        "games": games, "anticheat_blocked": anticheat}, ensure_ascii=False, separators=(",", ":")) + "\n")
    print(len(games), "games,", len(anticheat), "anti-cheat blocked")


if __name__ == "__main__":
    main(sys.argv[1])
