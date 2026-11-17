#!/usr/bin/env python3
"""Refresh site/anticheat.json from Are We Anti-Cheat Yet (MIT).

Keeps the games whose anti-cheat does not let them run under Wine/Proton (status Denied or
Broken), with their Steam app id when the list has one. site/mac_games.py marks those pages.
    python3 site/anticheat_update.py
"""
import json
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "site/anticheat.json"
SRC = "https://raw.githubusercontent.com/AreWeAntiCheatYet/AreWeAntiCheatYet/master/games.json"
LICENSE = "https://raw.githubusercontent.com/AreWeAntiCheatYet/AreWeAntiCheatYet/master/LICENSE"


def fetch(url):
    req = urllib.request.Request(url, headers={"User-Agent": "soju-site"})
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.read().decode()


def main():
    games = json.loads(fetch(SRC))
    notice = fetch(LICENSE).strip()
    out = {}
    for g in games:
        if g.get("status") not in ("Denied", "Broken") or g.get("native"):
            continue
        out[g["slug"]] = {"title": g["name"], "appid": (g.get("storeIds") or {}).get("steam"),
                          "anticheats": g.get("anticheats") or [], "status": g["status"]}
    data = {"_source": "https://areweanticheatyet.com/ (" + SRC + ")",
            "_license": "MIT, notice below", "_notice": notice,
            "games": dict(sorted(out.items()))}
    OUT.write_text(json.dumps(data, ensure_ascii=False, indent=1) + "\n")
    print(f"{len(out)} games blocked by anti-cheat -> {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
