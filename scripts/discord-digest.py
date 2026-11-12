#!/usr/bin/env python3
"""Posts to the Soju Discord from GitHub Actions (.github/workflows/discord.yml).

    discord-digest.py releases [--hours 24]   #announcements: what was released in that window
    discord-digest.py issue                   #compatibility: a new compatibility report (GITHUB_EVENT_PATH)
    discord-digest.py weekly                  #general: the week's releases and newly checked games

Each mode reads its webhook from the environment (DISCORD_ANNOUNCE_WEBHOOK,
DISCORD_COMPAT_WEBHOOK, DISCORD_GENERAL_WEBHOOK). Without it the text is printed
and nothing is sent. Nothing is posted when there is nothing to say.
Text from issues is untrusted: mentions are switched off and markup is flattened.
"""
import argparse
from datetime import datetime, timedelta, timezone
import json
import os
from pathlib import Path
import re
import sys
import urllib.request

REPO = os.environ.get("GITHUB_REPOSITORY", "BCD1210/soju")
ROOT = Path(__file__).resolve().parents[1]
SITE = "https://didimlabs.com"
LIMIT = 1900
APP_TAG = re.compile(r"^v\d+\.\d+\.\d+$")


def github(path):
    request = urllib.request.Request("https://api.github.com/repos/" + REPO + path,
                                     headers={"Accept": "application/vnd.github+json", "User-Agent": "soju-digest"})
    token = os.environ.get("GITHUB_TOKEN")
    if token:
        request.add_header("Authorization", "Bearer " + token)
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def when(text):
    return datetime.fromisoformat(text.replace("Z", "+00:00"))


def recent_releases(hours, now=None, releases=None):
    now = now or datetime.now(timezone.utc)
    releases = github("/releases?per_page=40") if releases is None else releases
    found = [r for r in releases if APP_TAG.match(r.get("tag_name", "")) and not r.get("draft")
             and r.get("published_at") and now - when(r["published_at"]) <= timedelta(hours=hours)]
    return sorted(found, key=lambda r: when(r["published_at"]))


def notes(body):
    """The bullet lines of a release body, without checksums."""
    lines = []
    for line in (body or "").replace("\r", "").split("\n"):
        line = line.rstrip()
        if not line.startswith(("- ", "* ", "• ")) or "SHA-256" in line:
            continue
        lines.append("• " + line[2:].strip())
    return lines


def clean(text, limit):
    """Untrusted text for Discord: one line per line, no mentions, no code fences."""
    text = re.sub(r"<!--.*?-->", "", text or "", flags=re.S).replace("\r", "")
    text = text.replace("@", "@​").replace("```", "'''")
    lines = [line.strip() for line in text.split("\n") if line.strip() and line.strip() != "_No response_"]
    out = "\n".join(lines)
    return out if len(out) <= limit else out[:limit].rsplit("\n", 1)[0] + "\n…"


def chunks(text):
    parts, current = [], ""
    for line in text.split("\n"):
        line = line[:LIMIT]
        if len(current) + len(line) + 1 > LIMIT:
            parts.append(current)
            current = line
        else:
            current = current + "\n" + line if current else line
    if current:
        parts.append(current)
    return parts[:4]


def releases_message(found):
    if not found:
        return ""
    latest = found[-1]["tag_name"].lstrip("v")
    out = ["**Soju " + latest + " is out**" if len(found) == 1 else
           "**Soju " + latest + " is out** (" + str(len(found)) + " releases since the last post)"]
    for release in reversed(found):
        lines = notes(release.get("body"))
        if not lines:
            continue
        if len(found) > 1:
            out.append("")
            out.append("**" + release["tag_name"].lstrip("v") + "**")
        out.extend(lines)
    out.append("")
    out.append("The app updates itself: Update and Restart in the sidebar. Download: " + SITE)
    return "\n".join(out)


def issue_message(event):
    issue = event.get("issue") or {}
    title = issue.get("title") or ""
    if event.get("action") != "opened" or not title.startswith("[Compatibility]"):
        return ""
    return ("**New compatibility report** from " + clean(issue.get("user", {}).get("login", "someone"), 60) + "\n"
            + clean(title.replace("[Compatibility]:", "").strip(), 200) + "\n"
            + clean(issue.get("body"), 900) + "\n<" + issue.get("html_url", "") + ">")


def weekly_message(found, games, open_reports, now=None):
    now = now or datetime.now(timezone.utc)
    week = (now - timedelta(days=7)).date().isoformat()
    checked = [g for g in games if g.get("status") == "verified" and (g.get("verified_on") or "") >= week]
    if not found and not checked:
        return ""
    out = ["**This week in Soju**"]
    if found:
        out.append("")
        out.append("**" + str(len(found)) + " release" + ("s" if len(found) > 1 else "") + ", now at "
                   + found[-1]["tag_name"].lstrip("v") + "**")
        for release in list(reversed(found))[:6]:
            first = notes(release.get("body"))[:1]
            if first:
                line = first[0]
                out.append(line if len(line) <= 230 else line[:230].rsplit(" ", 1)[0] + " …")
    if checked:
        out.append("")
        out.append("**Games we ran this week**")
        for game in checked:
            out.append("• " + game["name"] + ": <" + SITE + "/mac/" + game["slug"] + ".html>")
    out.append("")
    if open_reports:
        out.append(str(open_reports) + " game report" + ("s are" if open_reports > 1 else " is")
                   + " open: <https://github.com/" + REPO + "/issues?q=is%3Aissue+is%3Aopen+label%3Acompatibility>")
    out.append("Which game should we try next? Reply here.")
    return "\n".join(out)


def post(variable, text):
    if not text:
        print("Nothing to post.")
        return
    url = os.environ.get(variable)
    for part in chunks(text):
        if not url:
            print(part + "\n---")
            continue
        body = json.dumps({"content": part, "username": "Soju", "allowed_mentions": {"parse": []}, "flags": 4}).encode()
        request = urllib.request.Request(url + "?wait=true", data=body, method="POST",
                                         headers={"Content-Type": "application/json", "User-Agent": "soju-digest"})
        with urllib.request.urlopen(request, timeout=30) as response:
            print("Posted (" + str(response.status) + ").")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=["releases", "issue", "weekly"])
    parser.add_argument("--hours", type=int, default=24)
    args = parser.parse_args()
    if args.mode == "releases":
        post("DISCORD_ANNOUNCE_WEBHOOK", releases_message(recent_releases(args.hours)))
    elif args.mode == "issue":
        event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
        post("DISCORD_COMPAT_WEBHOOK", issue_message(event))
    else:
        data = json.loads((ROOT / "site/mac-games.json").read_text())
        games = data if isinstance(data, list) else data.get("games", [])
        reports = github("/issues?state=open&labels=compatibility&per_page=100")
        post("DISCORD_GENERAL_WEBHOOK", weekly_message(recent_releases(24 * 7), games,
                                                         len([i for i in reports if "pull_request" not in i])))


if __name__ == "__main__":
    try:
        main()
    except Exception as error:  # a failed post must be visible in the workflow run
        print("discord-digest: " + str(error), file=sys.stderr)
        sys.exit(1)
