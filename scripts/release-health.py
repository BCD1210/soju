#!/usr/bin/env python3
"""Checks that what users install still resolves.

    release-health.py [--wait SECONDS] [--grace-hours 3]

1. Every release asset this checkout pins (the engine in install.sh and
   scripts/update.sh, resources/*.json) answers on GitHub.
2. The Homebrew tap installs the newest stable release. Behind by a release
   older than the grace period is a failure (scripts/publish-release.sh moves it).
3. Every asset pinned by the version the tap installs still answers: `soju install`
   from Homebrew runs that version's installer, not the newest one.

A pinned engine or support release must stay on GitHub for as long as a version
that names it can still be installed. Run in CI (.github/workflows/check.yml).
--wait retries the tap check for that long (GitHub's raw cache lags a push).
"""
import argparse
from datetime import datetime, timedelta, timezone
import json
import os
from pathlib import Path
import re
import sys
import time
import urllib.error
import urllib.request

REPO = os.environ.get("SOJU_REPO", "BCD1210/soju")
TAP = os.environ.get("SOJU_TAP", "BCD1210/homebrew-soju")
ROOT = Path(__file__).resolve().parents[1]
APP_TAG = re.compile(r"^v\d+\.\d+\.\d+$")
RESOURCES = ["launcher-support", "steam-support", "steam-wine", "steam-dxvk", "gptk", "xbox-runtime", "d2r-vr", "didim-store"]


def fetch(url, method="GET", token=True):
    request = urllib.request.Request(url, method=method, headers={"User-Agent": "soju-release-health"})
    key = os.environ.get("GITHUB_TOKEN")
    if token and key and url.startswith("https://api.github.com/"):
        request.add_header("Authorization", "Bearer " + key)
    return urllib.request.urlopen(request, timeout=30)


def engine_urls(script_text):
    """The engine archive an installer script pins, from its ENGINE_TAG line."""
    found = re.search(r'^ENGINE_TAG="([^"]+)"', script_text, re.M)
    return ["https://github.com/%s/releases/download/%s/wine-engine-x86_64.tar.xz" % (REPO, found.group(1))] if found else []


def json_urls(text):
    """Every "url" value in a resources file."""
    urls = []

    def walk(node):
        if isinstance(node, dict):
            for key, value in node.items():
                if key == "url" and isinstance(value, str):
                    urls.append(value)
                else:
                    walk(value)
        elif isinstance(node, list):
            for value in node:
                walk(value)

    walk(json.loads(text))
    return urls


def own_release(url):
    return url.startswith("https://github.com/%s/releases/download/" % REPO)


def formula_version(text):
    found = re.search(r"archive/refs/tags/v(\d+\.\d+\.\d+)\.tar\.gz", text)
    return found.group(1) if found else ""


def newest_stable(releases):
    stable = [r for r in releases if APP_TAG.match(r.get("tag_name", "")) and not r.get("draft") and not r.get("prerelease")]
    stable.sort(key=lambda r: [int(n) for n in r["tag_name"][1:].split(".")])
    return stable[-1] if stable else None


def tap_is_late(tap_version, release, now, grace_hours):
    """Behind the newest release, and that release has been out longer than the grace period."""
    if not release or tap_version == release["tag_name"][1:]:
        return False
    published = datetime.fromisoformat(release["published_at"].replace("Z", "+00:00"))
    return now - published > timedelta(hours=grace_hours)


def missing(urls):
    gone = []
    for url in sorted(set(urls)):
        try:
            fetch(url, "HEAD").close()
        except urllib.error.HTTPError as error:
            gone.append("%s -> HTTP %s" % (url, error.code))
        except OSError as error:
            gone.append("%s -> %s" % (url, error))
    return gone


def pinned_here():
    urls = []
    for name in ("install.sh", "scripts/update.sh"):
        urls += engine_urls((ROOT / name).read_text())
    for path in sorted((ROOT / "resources").glob("*.json")):
        urls += [u for u in json_urls(path.read_text()) if own_release(u)]
    return urls


def pinned_at(version):
    raw = "https://raw.githubusercontent.com/%s/v%s/" % (REPO, version)
    urls = []
    for name in ("install.sh", "scripts/update.sh"):
        with fetch(raw + name) as response:
            urls += engine_urls(response.read().decode())
    for name in RESOURCES:
        try:
            with fetch(raw + "resources/%s.json" % name) as response:
                urls += [u for u in json_urls(response.read().decode()) if own_release(u)]
        except urllib.error.HTTPError:
            pass  # that version had no such file
    return urls


def tap_version():
    with fetch("https://api.github.com/repos/%s/contents/Formula/soju.rb" % TAP) as response:
        import base64
        return formula_version(base64.b64decode(json.load(response)["content"]).decode())


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--wait", type=int, default=0)
    parser.add_argument("--grace-hours", type=float, default=3)
    args = parser.parse_args()
    problems = ["Pinned here, gone from GitHub: " + line for line in missing(pinned_here())]
    with fetch("https://api.github.com/repos/%s/releases?per_page=40" % REPO) as response:
        newest = newest_stable(json.load(response))
    deadline = time.time() + args.wait
    while True:
        tap = tap_version()
        if not newest or tap == newest["tag_name"][1:] or time.time() >= deadline:
            break
        time.sleep(10)
    if not tap:
        problems.append("Could not read the version in %s Formula/soju.rb" % TAP)
    else:
        grace = 0 if args.wait else args.grace_hours
        if tap_is_late(tap, newest, datetime.now(timezone.utc), grace):
            problems.append("Homebrew installs %s but the newest release is %s: run scripts/update-homebrew-tap.sh %s"
                            % (tap, newest["tag_name"][1:], newest["tag_name"][1:]))
        problems += ["Homebrew's %s pins a file that is gone: %s" % (tap, line) for line in missing(pinned_at(tap))]
    for line in problems:
        print("FAIL " + line)
    if not problems:
        print("ok   pinned assets answer; Homebrew installs %s (newest release %s)" % (tap, newest["tag_name"][1:] if newest else "none"))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
