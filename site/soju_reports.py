#!/usr/bin/env python3
"""Collect Soju users' compatibility reports for the game pages.

Reads the public GitHub issues labelled "compatibility" (the form that
`soju report` and the app's Report How It Ran open), takes the answers out of
the form, and writes site/soju-reports.json. site/mac_games.py shows them on
each game's page. Run it before building the pages:

    python3 site/soju_reports.py && python3 site/mac_games.py

Only what the reporter wrote is used: game, store, result, Mac, macOS, Soju
version, frame rate and play time when given, and the issue link.
"""
import json
import re
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "site/soju-reports.json"
API = "https://api.github.com/repos/BCD1210/soju/issues?labels=compatibility&state=all&per_page=100&page={}"
FIELDS = {"Game": "game", "Store": "store", "Result": "result", "Mac model, chip and memory": "mac",
          "macOS version": "macos", "Soju version": "soju", "Frame rate": "fps", "How long you played": "played"}
WORKS = "Works in-game"


def sections(body):
    """The issue form writes '### Label' followed by the answer."""
    out, label = {}, None
    for line in (body or "").splitlines():
        m = re.match(r"^###\s+(.+?)\s*$", line)
        if m:
            label = m.group(1)
            continue
        if label in FIELDS and line.strip() and line.strip() != "_No response_":
            key = FIELDS[label]
            out[key] = (out.get(key, "") + " " + line.strip()).strip()
    return out


def parse(issue):
    f = sections(issue.get("body"))
    if not f.get("game") or not f.get("result"):
        return None
    appid = (re.search(r"App ?ID[:\s]*(\d{2,8})", f["game"], re.I) or re.search(r"\bapp\s+(\d{2,8})\b", f["game"], re.I)
             or re.search(r"store\.steampowered\.com/app/(\d+)", issue.get("body") or ""))
    name = re.sub(r"\s*\(.*?\)\s*", " ", f["game"]).strip()
    title = re.sub(r"^\[Compatibility\]:\s*", "", issue.get("title") or "").strip()
    # Maintainers retitle a report with what actually happened; a title that
    # says it failed outranks a "Works in-game" picked by mistake.
    failed = re.search(r"cannot|can't|\bfails?\b|crash|white window|black screen|freez|\bexits\b|\bhangs?\b|never|not (start|launch|work)",
                       title, re.I)
    return {"number": issue["number"], "url": issue["html_url"], "date": issue["created_at"][:10],
            "name": name, "appid": appid.group(1) if appid else None, "title": title[:160],
            # The form's choice may be followed by a note ("Works in-game: played 12 minutes ...").
            "works": f["result"].startswith(WORKS) and not failed,
            **{k: f[k][:120] for k in ("store", "result", "mac", "macos", "soju", "fps", "played") if f.get(k)}}


def main():
    reports, page = [], 1
    while True:
        req = urllib.request.Request(API.format(page), headers={"Accept": "application/vnd.github+json",
                                                                "User-Agent": "soju-site"})
        with urllib.request.urlopen(req, timeout=30) as r:
            batch = json.load(r)
        for issue in batch:
            if "pull_request" in issue or issue.get("state_reason") == "not_planned":
                continue
            rep = parse(issue)
            if rep:
                reports.append(rep)
        if len(batch) < 100:
            break
        page += 1
    reports.sort(key=lambda r: r["number"])
    OUT.write_text(json.dumps({"_source": "GitHub issues labelled compatibility in BCD1210/soju",
                               "reports": reports}, ensure_ascii=False, indent=1) + "\n")
    print(len(reports), "reports")


if __name__ == "__main__":
    main()
