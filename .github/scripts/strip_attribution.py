#!/usr/bin/env python3
"""Remove AI tool attribution from issue, pull request and comment text.

  strip_attribution.py event          clean the item in $GITHUB_EVENT_PATH
  strip_attribution.py sweep          clean every issue, PR and comment (daily too)
  strip_attribution.py commits A..B   fail if a commit in the range carries it

Only text written by the repository's owner, members and collaborators is
edited; other people's words are never touched.
"""
import json
import os
import re
import subprocess
import sys
import urllib.error
import urllib.request

LINES = [re.compile(p, re.I) for p in (
    r"^\s*🤖\s*Generated with \[Claude Code\]\([^)]*\)\s*$",
    r"^\s*_?Generated (?:by|with) \[Claude Code\]\([^)]*\)_?\s*$",
    r"^\s*<?https://claude\.ai/code/session_\w+>?\s*$",
    r"^\s*Co-Authored-By:\s*Claude\b.*$",
    r"^\s*Claude-Session:.*$",
)]
TRUSTED = {"OWNER", "MEMBER", "COLLABORATOR"}


def clean(text):
    """Return text without attribution lines, or the same text if it has none."""
    if not text:
        return text
    lines = text.splitlines()
    marked = [any(p.match(line) for p in LINES) for line in lines]
    if not any(marked):
        return text
    # The footer is everything after the last real line, when attribution
    # sits there: its blank lines and "---" rule go with it. A rule that
    # ends the text on its own, with no attribution after it, stays.
    end = len(lines)
    while end and (marked[end - 1] or lines[end - 1].strip() in ("", "---")):
        end -= 1
    if not any(marked[end:]):
        end = len(lines)
    return "\n".join(line for line, m in zip(lines[:end], marked) if not m)


def attributed(message):
    return any(p.match(line) for line in message.splitlines() for p in LINES)


def api(method, path, body=None):
    url = path if path.startswith("https://") else "https://api.github.com" + path
    data = json.dumps(body).encode() if body is not None else None
    request = urllib.request.Request(url, data=data, method=method, headers={
        "Authorization": "Bearer " + os.environ["GITHUB_TOKEN"],
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
    })
    with urllib.request.urlopen(request, timeout=30) as response:
        link = response.headers.get("Link", "")
        following = re.search(r'<([^>]+)>;\s*rel="next"', link)
        return json.loads(response.read() or "null"), following and following[1]


def pages(path):
    url = path
    while url:
        items, url = api("GET", url)
        yield from items


WRITERS = {"admin", "maintain", "write"}
_permissions = {}


def trusted(item):
    """Owner, member or collaborator, or anyone with write access.

    author_association depends on who asks: the Actions token cannot see
    private organization membership, so a member's comments list as
    CONTRIBUTOR there. Their repository permission settles it.
    """
    if item.get("author_association") in TRUSTED:
        return True
    login = (item.get("user") or {}).get("login")
    if not login or (item.get("user") or {}).get("type") == "Bot":
        return False
    if login not in _permissions:
        repo = "/repos/" + os.environ["GITHUB_REPOSITORY"]
        try:
            result, _ = api("GET", f"{repo}/collaborators/{login}/permission")
            _permissions[login] = result.get("permission")
        except urllib.error.HTTPError as error:
            if error.code not in (403, 404):
                raise
            _permissions[login] = None
    return _permissions[login] in WRITERS


def fix(item, edit_url):
    """Edit one issue, PR or comment if a trusted author left attribution in it."""
    if clean(item.get("body")) == item.get("body"):
        return False
    if not trusted(item):
        print("skipped", item.get("html_url", edit_url), "by",
              (item.get("user") or {}).get("login"), item.get("author_association"))
        return False
    # The payload may predate a newer edit: clean the body as it is now.
    item, _ = api("GET", edit_url)
    body = item.get("body") or ""
    cleaned = clean(body)
    if cleaned == body:
        return False
    try:
        api("PATCH", edit_url, {"body": cleaned})
    except urllib.error.HTTPError as error:
        if error.code != 403:
            raise
        # Fork PR events get a read-only token; the daily sweep cleans it.
        print("::notice::No write access for", item.get("html_url", edit_url), "- left for the daily sweep")
        return False
    print("cleaned", item.get("html_url", edit_url))
    return True


def event():
    with open(os.environ["GITHUB_EVENT_PATH"], encoding="utf-8") as stream:
        payload = json.load(stream)
    repo = "/repos/" + os.environ["GITHUB_REPOSITORY"]
    if "comment" in payload:
        comment = payload["comment"]
        kind = "pulls/comments" if "pull_request_review_id" in comment else "issues/comments"
        fix(comment, f"{repo}/{kind}/{comment['id']}")
    else:
        item = payload.get("issue") or payload.get("pull_request")
        if item:
            fix(item, f"{repo}/issues/{item['number']}")


def sweep():
    repo = "/repos/" + os.environ["GITHUB_REPOSITORY"]
    count = 0
    for item in pages(f"{repo}/issues?state=all&per_page=100"):
        count += fix(item, f"{repo}/issues/{item['number']}")
    for comment in pages(f"{repo}/issues/comments?per_page=100"):
        count += fix(comment, f"{repo}/issues/comments/{comment['id']}")
    for comment in pages(f"{repo}/pulls/comments?per_page=100"):
        count += fix(comment, f"{repo}/pulls/comments/{comment['id']}")
    print(f"cleaned {count} item(s)")


def commits(revisions):
    log = subprocess.run(["git", "log", "--format=%H%x00%B%x01", revisions],
                         check=True, capture_output=True, text=True).stdout
    bad = [entry.split("\0", 1)[0][:12] for entry in log.split("\1")
           if "\0" in entry and attributed(entry.split("\0", 1)[1])]
    for sha in bad:
        print(f"::error::Commit {sha} carries AI attribution (Co-Authored-By: Claude or Claude-Session); "
              "reword it before merging.")
    return 1 if bad else 0


if __name__ == "__main__":
    mode = sys.argv[1] if len(sys.argv) > 1 else ""
    if mode == "event":
        event()
    elif mode == "sweep":
        sweep()
    elif mode == "commits" and len(sys.argv) == 3:
        sys.exit(commits(sys.argv[2]))
    else:
        sys.exit(__doc__)
