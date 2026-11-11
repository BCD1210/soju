#!/usr/bin/env python3
"""Give installed Steam games the renderer they are known to run best on.

    steam-profiles.py apply      switch matching games (play.sh runs this before Steam starts)
    steam-profiles.py list       show what would change, change nothing

resources/steam-profiles.json lists Direct3D 11 games that run better on DXVK
than on the default DXMT. A game is switched once, through steam-renderer.py
(which keeps the original files and can reset them). A game the player already
set up with `soju steam-games renderer`, or switched back, is left alone: the
choice is recorded in <bottle>/.soju-auto-profiles.json.
"""
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
BASE = Path(os.environ.get("SOJU_BASE", str(Path.home() / ".battlenet-macos"))).expanduser()
PREFIX = Path(os.environ.get("WINEPREFIX", str(BASE / "steam-bottle"))).expanduser()
SKIP = re.compile(r"crash|unins|setup|redist|dxsetup|prereq|launcher|helper|report|easyanticheat|beservice|"
                  r"update|install|vc_|dotnet|ue4prereq|unitycrash|cefprocess|webhelper", re.I)


def is_x64(path):
    try:
        with path.open("rb") as f:
            if f.read(2) != b"MZ":
                return False
            f.seek(0x3c)
            f.seek(struct.unpack("<I", f.read(4))[0])
            return f.read(6) == b"PE\0\0\x64\x86"
    except (OSError, struct.error):
        return False


def main_exe(folder):
    """The biggest 64-bit .exe near the top of the game folder that is not a helper."""
    found = []
    for depth in range(4):
        for exe in folder.glob("/".join(["*"] * depth + ["*.exe"])):
            if exe.is_file() and not exe.is_symlink() and not SKIP.search(exe.name) and is_x64(exe):
                found.append(exe)
    return max(found, key=lambda p: p.stat().st_size) if found else None


def installed(steamapps):
    for acf in steamapps.glob("appmanifest_*.acf"):
        text = acf.read_text(errors="replace")
        appid = re.search(r'"appid"\s+"(\d+)"', text)
        folder = re.search(r'"installdir"\s+"([^"]+)"', text)
        if appid and folder:
            yield appid.group(1), steamapps / "common" / folder.group(1)


def plan():
    profiles = json.loads((ROOT / "resources/steam-profiles.json").read_text())["profiles"]
    steamapps = PREFIX / "drive_c/Program Files (x86)/Steam/steamapps"
    state_file = PREFIX / ".soju-auto-profiles.json"
    state = json.loads(state_file.read_text()) if state_file.is_file() else {}
    managed = PREFIX / ".soju-renderers"
    todo = []
    for appid, folder in installed(steamapps) if steamapps.is_dir() else []:
        p = profiles.get(appid)
        if not p or appid in state or not folder.is_dir():
            continue
        exe = main_exe(folder)
        if exe is None:
            continue
        # A profile the player made by hand for this executable wins.
        if managed.is_dir() and any(json.loads(r.read_text()).get("game") == str(exe.resolve())
                                    for r in managed.glob("*.json")):
            state[appid] = {"skipped": "set by the player"}
            continue
        todo.append((appid, p, exe))
    return todo, state, state_file


def main():
    action = sys.argv[1] if len(sys.argv) > 1 else "list"
    todo, state, state_file = plan()
    if action == "list":
        for appid, p, exe in todo:
            print(f"{p['title']} ({appid}): {p['renderer']} for {exe.name} - {p['why']}")
        return
    if action != "apply":
        sys.exit("usage: steam-profiles.py apply|list")
    if not (PREFIX / ".soju-steam-games").is_file():
        return   # renderer components not prepared yet (soju steam-games)
    for appid, p, exe in todo:
        r = subprocess.run([sys.executable, str(ROOT / "scripts/steam-renderer.py"), p["renderer"], str(exe)],
                           capture_output=True, text=True)
        if r.returncode == 0:
            state[appid] = {"renderer": p["renderer"], "exe": str(exe)}
            print(f"soju: {p['title']} set to {p['renderer'].upper()} ({p['why']}) "
                  f"Undo: soju steam-games renderer reset \"{exe}\"")
        else:
            print(f"soju: could not set {p['title']} to {p['renderer']}: {(r.stderr or r.stdout).strip()[-200:]}",
                  file=sys.stderr)
    if state:
        state_file.write_text(json.dumps(state, indent=1) + "\n")


if __name__ == "__main__":
    main()
