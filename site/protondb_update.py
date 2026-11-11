#!/usr/bin/env python3
"""Refresh site/protondb-estimates.json from a ProtonDB reports dump.

ProtonDB's reports are published under the Open Database License (ODbL) at
https://github.com/bdefore/protondb-data. This keeps, per Steam app, the player
reports of the last two years that give a verdict, and writes
[appid, title, reports, reports_that_run, newest_report_unix_time] for apps with
at least 3 such reports. The output is a derived database under the ODbL too.

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
    agg = collections.defaultdict(lambda: [None, 0, 0, 0])
    for r in reports:
        try:
            app = str(int(r["app"]["steam"]["appId"]))
        except (KeyError, TypeError, ValueError):
            continue
        verdict = (r.get("responses") or {}).get("verdict")
        if r.get("timestamp", 0) < cutoff or verdict not in ("yes", "no"):
            continue
        a = agg[app]
        a[0] = (r["app"].get("title") or a[0] or "").strip()
        a[1] += 1
        a[2] += verdict == "yes"
        a[3] = max(a[3], r["timestamp"])
    rows = sorted(([app, t, n, y, last] for app, (t, n, y, last) in agg.items() if n >= 3 and t),
                  key=lambda r: (-r[2], r[1]))
    OUT.write_text(json.dumps({
        "_license": "Derived from ProtonDB reports (https://github.com/bdefore/protondb-data), Open Database License (ODbL) 1.0.",
        "_fields": ["steam_appid", "title", "reports_last_2_years", "reports_saying_it_runs", "newest_report_unix_time"],
        "dump_newest_report": time.strftime("%Y-%m-%d", time.gmtime(newest)),
        "apps": rows}, ensure_ascii=False, separators=(",", ":")) + "\n")
    print(len(rows), "apps")


if __name__ == "__main__":
    main(sys.argv[1])
