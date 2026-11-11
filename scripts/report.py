#!/usr/bin/env python3
"""Open a compatibility report on GitHub with this Mac's details filled in.

  soju report [<library id> | <game title>]

The page opens in the browser; nothing is sent until you press Submit there.
Fills: game, store, Mac model / chip / memory, macOS, Soju version, engine.
Reports feed the game pages at didimlabs.com/mac/.
"""
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import urllib.parse

ROOT = Path(__file__).resolve().parent.parent
BASE = Path(os.environ.get("SOJU_BASE", str(Path.home() / ".battlenet-macos")))
NEW = "https://github.com/BCD1210/soju/issues/new"
STORES = {"battlenet": "Battle.net", "steam": "Steam", "epic": "Epic Games Launcher", "gog": "GOG GALAXY",
          "ea": "EA app", "rockstar": "Rockstar Games Launcher", "ubisoft": "Ubisoft Connect", "xbox": "Xbox PC"}


def run(*cmd):
    try:
        return subprocess.run(cmd, capture_output=True, text=True, timeout=10).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return ""


def mac():
    model = run("/usr/sbin/sysctl", "-n", "hw.model")
    chip = run("/usr/sbin/sysctl", "-n", "machdep.cpu.brand_string")
    try:
        mem = "%d GB" % round(int(run("/usr/sbin/sysctl", "-n", "hw.memsize")) / 2**30)
    except ValueError:
        mem = ""
    return ", ".join(x for x in (model, chip, mem) if x)


def engine():
    parts = []
    e = BASE / "cx26-engine"
    if (e / "bin/wine").exists():
        parts.append("CX engine " + ((e / ".soju-engine-release").read_text().strip()
                                     if (e / ".soju-engine-release").exists() else "unknown"))
        parts.append("D3DMetal" if (e / "lib/external/libd3dshared.dylib").exists() else "no GPTK")
    if (BASE / "cx26-engine-launchers/.soju-launcher-engine").exists():
        parts.append("launcher engine " + (BASE / "cx26-engine-launchers/.soju-launcher-engine").read_text().strip())
    if (BASE / "steam-support/prebuilt/.soju-support-version").exists():
        parts.append("Steam " + (BASE / "steam-support/prebuilt/.soju-support-version").read_text().strip())
    return "; ".join(parts) or "unknown"


def find(arg):
    """A library id (battlenet:osi) or a title gives the game and its store."""
    if not arg:
        return "", ""
    try:
        out = run("python3", str(ROOT / "scripts/game-library.py"), "list")
        for g in json.loads(out).get("games", []):
            if arg in (g.get("id"), g.get("title")) or arg.lower() == (g.get("title") or "").lower():
                return g.get("title", arg), STORES.get(g.get("platform"), "")
    except ValueError:
        pass
    if ":" in arg and arg.split(":", 1)[0] in STORES:
        return arg.split(":", 1)[1], STORES[arg.split(":", 1)[0]]
    return arg, ""


def main():
    title, store = find(" ".join(sys.argv[1:]).strip())
    fields = {"template": "compatibility.yml", "title": "[Compatibility]: " + title, "game": title,
              "mac": mac(), "macos": platform.mac_ver()[0], "engine": engine(),
              "soju": (ROOT / "VERSION").read_text().strip() if (ROOT / "VERSION").exists() else "unknown"}
    if store:
        fields["store"] = store
    url = NEW + "?" + urllib.parse.urlencode({k: v for k, v in fields.items() if v})
    if os.environ.get("SOJU_REPORT_PRINT") == "1":
        print(url)
        return
    subprocess.run(["/usr/bin/open", url], check=False)
    print("Opened the report form in your browser. Nothing is sent until you press Submit there.")


if __name__ == "__main__":
    main()
