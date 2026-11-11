#!/usr/bin/env python3
"""Refresh site/protondb-estimates.json from a ProtonDB reports dump.

ProtonDB's reports are published under the Open Database License (ODbL) at
https://github.com/bdefore/protondb-data. This keeps, per Steam app, the player
reports that give a verdict: those of the last two years when there are at least
3, otherwise all of them, and writes
[appid, title, reports, reports_that_run, from_last_2_years, newest_report_unix_time]. The output is a derived database under the ODbL too.

  python3 site/protondb_update.py reports_piiremoved.json
"""
import collections
import json
from pathlib import Path
import sys
import time

OUT = Path(__file__).resolve().parent / "protondb-estimates.json"


def main(path):
    reports = json.load(open(path))
    newest = max(r.get("timestamp", 0) for r in reports)
    cutoff = newest - 2 * 365 * 86400
    recent = collections.defaultdict(lambda: [0, 0])
    total = collections.defaultdict(lambda: [None, 0, 0, 0])
    for r in reports:
        try:
            app = str(int(r["app"]["steam"]["appId"]))
        except (KeyError, TypeError, ValueError):
            continue
        verdict = (r.get("responses") or {}).get("verdict")
        if verdict not in ("yes", "no"):
            continue
        t = total[app]
        t[0] = (r["app"].get("title") or t[0] or "").strip()
        t[1] += 1
        t[2] += verdict == "yes"
        t[3] = max(t[3], r.get("timestamp", 0))
        if r.get("timestamp", 0) >= cutoff:
            recent[app][0] += 1
            recent[app][1] += verdict == "yes"
    rows = []
    for app, (title, n, y, last) in total.items():
        if not title:
            continue
        rn, ry = recent.get(app, (0, 0))
        # Recent reports when there are enough of them; otherwise every report there is.
        rows.append([app, title, rn, ry, 1, last] if rn >= 3 else [app, title, n, y, 0, last])
    rows.sort(key=lambda r: (-r[2], r[1]))
    OUT.write_text(json.dumps({
        "_license": "Derived from ProtonDB reports (https://github.com/bdefore/protondb-data), Open Database License (ODbL) 1.0.",
        "_fields": ["steam_appid", "title", "reports", "reports_saying_it_runs", "reports_are_from_last_2_years", "newest_report_unix_time"],
        "dump_newest_report": time.strftime("%Y-%m-%d", time.gmtime(newest)),
        "apps": rows}, ensure_ascii=False, separators=(",", ":")) + "\n")
    print(len(rows), "apps")


if __name__ == "__main__":
    main(sys.argv[1])
