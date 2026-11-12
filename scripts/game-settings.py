#!/usr/bin/env python3
"""Settings of one library game, for the app's game sheet.

    game-settings.py show <game id>                 JSON: what can be set and its value
    game-settings.py renderer <game id> dxmt|dxvk   Steam games: graphics translation

Only Steam games in the default (Wine 11) Steam bottle have a setting today: the
Direct3D 11 translation, DXMT (default) or DXVK. The work is done by
steam-renderer.py, which keeps the original files. A choice made here is recorded
in <bottle>/.soju-auto-profiles.json, so steam-profiles.py does not switch it back.
"""
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
BASE = Path(os.environ.get("SOJU_BASE", str(Path.home() / ".battlenet-macos"))).expanduser()
PREFIX = BASE / "steam-bottle"
spec = importlib.util.spec_from_file_location("steam_profiles", ROOT / "scripts/steam-profiles.py")
profiles = importlib.util.module_from_spec(spec)
spec.loader.exec_module(profiles)


def gptk_mode():
    return (BASE / "steam-gptk").is_file() and \
        (BASE / "steam-gptk-bottle/drive_c/Program Files (x86)/Steam/steam.exe").is_file()


def steam_exe(appid):
    """The game's main 64-bit .exe in the default Steam bottle, or None."""
    steamapps = PREFIX / "drive_c/Program Files (x86)/Steam/steamapps"
    for found, folder in profiles.installed(steamapps) if steamapps.is_dir() else []:
        if found == appid and folder.is_dir():
            return profiles.main_exe(folder)
    return None


def renderer_tool(mode, exe):
    env = dict(os.environ, WINEPREFIX=str(PREFIX))
    return subprocess.run([sys.executable, str(ROOT / "scripts/steam-renderer.py"), mode, str(exe)],
                          capture_output=True, text=True, env=env)


def show(game):
    platform, _, key = game.partition(":")
    result = {"game": game, "renderer": None}
    if platform != "steam" or not re.fullmatch(r"\d{1,10}", key):
        return result
    if gptk_mode():
        result["renderer"] = {"available": False, "reason": "gptk"}
        return result
    if not (PREFIX / ".soju-steam-games").is_file():
        return result
    exe = steam_exe(key)
    if exe is None:
        result["renderer"] = {"available": False, "reason": "no-64-bit-exe"}
        return result
    status = renderer_tool("status", exe)
    if status.returncode != 0:
        result["renderer"] = {"available": False, "reason": (status.stderr or status.stdout).strip()[-200:]}
        return result
    value = json.loads(status.stdout).get("renderer")
    known = json.loads((ROOT / "resources/steam-profiles.json").read_text())["profiles"].get(key)
    result["renderer"] = {"available": True, "value": "dxvk" if value == "dxvk" else "dxmt",
                          "recommended": known["renderer"] if known else None,
                          "why": known["why"] if known else None}
    return result


def set_renderer(game, mode):
    platform, _, key = game.partition(":")
    if platform != "steam" or not re.fullmatch(r"\d{1,10}", key) or mode not in ("dxmt", "dxvk"):
        raise ValueError("usage: game-settings.py renderer steam:<appid> dxmt|dxvk")
    if gptk_mode():
        raise ValueError("Steam is in Overwatch and Diablo II mode, where games use Apple GPTK.")
    exe = steam_exe(key)
    if exe is None:
        raise ValueError("This game has no 64-bit program Soju can set up.")
    # DXMT is the bottle's default, so choosing it means removing a DXVK profile.
    done = renderer_tool("dxvk" if mode == "dxvk" else "reset", exe)
    if done.returncode != 0:
        raise ValueError((done.stderr or done.stdout).strip().replace("Steam renderer: ", "")[-300:])
    state_file = PREFIX / ".soju-auto-profiles.json"
    state = json.loads(state_file.read_text()) if state_file.is_file() else {}
    state[key] = {"skipped": "set by the player", "renderer": mode}
    state_file.write_text(json.dumps(state, indent=1) + "\n")
    return show(game)


def main():
    if len(sys.argv) == 3 and sys.argv[1] == "show":
        print(json.dumps(show(sys.argv[2])))
    elif len(sys.argv) == 4 and sys.argv[1] == "renderer":
        print(json.dumps(set_renderer(sys.argv[2], sys.argv[3])))
    else:
        raise ValueError("usage: game-settings.py show <game id> | renderer <game id> dxmt|dxvk")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as error:
        print("soju: " + str(error), file=sys.stderr)
        sys.exit(1)
