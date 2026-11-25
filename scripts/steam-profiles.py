#!/usr/bin/env python3
"""Give installed Steam games the renderer they are known to run best on.

    steam-profiles.py apply      switch matching games (play.sh runs this before Steam starts)
    steam-profiles.py list       show what would change, change nothing
    steam-profiles.py nwjs       NW.js games (RPG Maker MV/MZ): draw in the game's own process
    steam-profiles.py options    launch options known games need (resources/steam-launch-options.json)

resources/steam-profiles.json lists Direct3D 11 games that run better on DXVK
than on the default DXMT. A game is switched once, through steam-renderer.py
(which keeps the original files and can reset them). A game the player already
set up with `soju steam-games renderer`, or switched back, is left alone: the
choice is recorded in <bottle>/.soju-auto-profiles.json.

NW.js games (RPG Maker MV and MZ ship as nw.dll + package.json next to Game.exe)
open an empty window: Chromium draws in a separate GPU process, and DXMT cannot
make a swap chain for a window that belongs to another process. `nwjs` adds
--in-process-gpu to the game's "chromium-args" (the original package.json is kept
as package.json.soju-orig). Steam restores the file on updates, so it runs before
every Steam start.
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


NWJS_FLAG = "--in-process-gpu"


def nwjs_games(steamapps):
    """package.json files of installed NW.js games: next to nw.dll, at most two folders deep."""
    for _, folder in installed(steamapps) if steamapps.is_dir() else []:
        for depth in range(3):
            for dll in folder.glob("/".join(["*"] * depth + ["nw.dll"])):
                manifest = dll.parent / "package.json"
                if manifest.is_file() and not manifest.is_symlink():
                    yield manifest


def nwjs_fix(manifest):
    """Add NWJS_FLAG to chromium-args; True when the file changed."""
    try:
        data = json.loads(manifest.read_text(encoding="utf-8-sig"))
    except (OSError, ValueError):
        return False
    if not isinstance(data, dict):
        return False
    args = data.get("chromium-args", "")
    if not isinstance(args, str) or NWJS_FLAG in args.split():
        return False
    backup = manifest.with_name("package.json.soju-orig")
    if not backup.exists():
        backup.write_bytes(manifest.read_bytes())
    data["chromium-args"] = (args + " " + NWJS_FLAG).strip()
    manifest.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return True


OPTIONS_FILE = ROOT / "resources/steam-launch-options.json"


def vdf_tokens(text):
    """(kind, value, start, end) for the quoted strings and braces of a Valve KeyValues text."""
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if c in " \t\r\n":
            i += 1
        elif text.startswith("//", i):
            i = text.find("\n", i) if text.find("\n", i) != -1 else n
        elif c in "{}":
            yield c, c, i, i + 1
            i += 1
        elif c == '"':
            j, out = i + 1, []
            while j < n and text[j] != '"':
                if text[j] == "\\" and j + 1 < n:
                    out.append(text[j + 1]); j += 2
                else:
                    out.append(text[j]); j += 1
            yield "s", "".join(out), i, j + 1
            i = j + 1
        else:   # an unquoted word
            j = i
            while j < n and text[j] not in " \t\r\n{}\"":
                j += 1
            yield "s", text[i:j], i, j


def vdf_tree(text):
    """Blocks by lower-case key: {"open", "close", "depth", "blocks": {...}, "values": {key: value}}."""
    toks = list(vdf_tokens(text))

    def block(k, depth, open_at):
        node = {"open": open_at, "close": len(text), "depth": depth, "blocks": {}, "values": {}}
        while k < len(toks):
            kind, value, start, _ = toks[k]
            if kind == "}":
                node["close"] = start
                return node, k + 1
            if kind == "s" and k + 1 < len(toks) and toks[k + 1][0] == "{":
                child, k = block(k + 2, depth + 1, toks[k + 1][2])
                node["blocks"].setdefault(value.lower(), child)
                continue
            if kind == "s" and k + 1 < len(toks) and toks[k + 1][0] == "s":
                node["values"][value.lower()] = toks[k + 1][1]
                k += 2
                continue
            k += 1
        return node, k

    return block(0, 0, -1)[0]


def with_launch_options(text, appid, options):
    """The localconfig.vdf text with LaunchOptions for appid, or None when the player already has some
    (or the file has no apps section to put them in)."""
    node = vdf_tree(text)
    for key in ("userlocalconfigstore", "software", "valve", "steam", "apps"):
        node = node["blocks"].get(key)
        if node is None:
            return None
    quoted = '"' + options.replace("\\", "\\\\").replace('"', '\\"') + '"'
    app = node["blocks"].get(appid)
    if app is not None:
        if app["values"].get("launchoptions", "").strip():
            return None
        at = app["open"] + 1
        return text[:at] + "\n" + "\t" * app["depth"] + '"LaunchOptions"\t\t' + quoted + text[at:]
    tabs = "\t" * (node["depth"] + 1)
    entry = tabs + '"' + appid + '"\n' + tabs + "{\n" + tabs + '\t"LaunchOptions"\t\t' + quoted + "\n" + tabs + "}\n"
    at = node["close"]
    line = text.rfind("\n", 0, at) + 1
    return text[:line] + entry + text[line:]


def set_launch_options(prefix=None, table=None):
    """Give installed games in resources/steam-launch-options.json their options, once per game and
    Steam account, while Steam is closed (Steam writes localconfig.vdf back when it quits)."""
    prefix = Path(prefix or PREFIX)
    steam = prefix / "drive_c/Program Files (x86)/Steam"
    wanted = table if table is not None else json.loads(OPTIONS_FILE.read_text())["options"]
    have = {appid for appid, _ in installed(steam / "steamapps")} if (steam / "steamapps").is_dir() else set()
    state_file = prefix / ".soju-launch-options.json"
    state = json.loads(state_file.read_text()) if state_file.is_file() else {}
    changed = []
    for config in sorted(steam.glob("userdata/*/config/localconfig.vdf")):
        user = config.parent.parent.name
        try:
            text = config.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        original = text
        for appid, item in wanted.items():
            key = user + ":" + appid
            if appid not in have or key in state:
                continue
            new = with_launch_options(text, appid, item["options"])
            state[key] = item["options"] if new is not None else "kept the player's"
            if new is not None:
                text = new
                changed.append(item)
        if text != original:
            backup = config.with_name("localconfig.vdf.soju-bak")
            if not backup.exists():
                backup.write_bytes(config.read_bytes())
            config.write_text(text, encoding="utf-8")
    if state:
        state_file.write_text(json.dumps(state, indent=1) + "\n")
    for item in changed:
        print(f"soju: {item['title']}: launch options {item['options']} ({item['why']})")
    return changed


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
    if action == "options":
        set_launch_options()
        return
    if action == "nwjs":
        for manifest in nwjs_games(PREFIX / "drive_c/Program Files (x86)/Steam/steamapps"):
            if nwjs_fix(manifest):
                print(f"soju: {manifest.parent.name}: NW.js game set to draw in its own process ({NWJS_FLAG})")
        return
    todo, state, state_file = plan()
    if action == "list":
        for appid, p, exe in todo:
            print(f"{p['title']} ({appid}): {p['renderer']} for {exe.name} - {p['why']}")
        return
    if action != "apply":
        sys.exit("usage: steam-profiles.py apply|list|nwjs|options")
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
