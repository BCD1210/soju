#!/usr/bin/env python3
"""Keep a Battle.net game's files on another drive.

    game-storage.py status <game id>             JSON: where the game's files are
    game-storage.py move <game id> <folder>      copy to <folder>/Soju Games/..., verify, leave a link
    game-storage.py back <game id>               bring the files back into the bottle
    game-storage.py place battlenet "<Game folder name>" <folder>
        Before installing (or to reuse files you already have): make
        C:\\Program Files (x86)\\<Game folder name> point at <folder>/Soju Games/battlenet/<name>.
        Then press Install in Battle.net and keep its default location; it finds files that
        are already there and downloads only what is missing.

    game-storage.py import battlenet <game folder>
        Use a game folder you already have (from another Wine app or a backup) where it is,
        without copying. Then press Install or Locate in Battle.net.

The bottle itself (Windows registry, launcher, links) stays on the internal disk: drives
formatted exFAT cannot hold the links and permissions a bottle needs. Only the game's own
folder moves, and Wine reaches it through a link. The drive has to be connected to play.
"""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

BASE = Path(os.environ.get("SOJU_BASE", str(Path.home() / ".battlenet-macos"))).expanduser()
ROOT = Path(__file__).resolve().parents[1]
BOTTLES = {"battlenet": "bottle"}
CLIENT_EXE = {"battlenet": "Battle.net.exe"}
FILESYSTEMS = ("apfs", "hfs", "exfat")
MARK = ".soju-game.json"


def fail(message):
    raise ValueError(message)


def mount_of(path):
    """(mount point, filesystem type) of the volume holding path."""
    out = subprocess.run(["/bin/df", "-P", str(path)], capture_output=True, text=True, check=True).stdout.splitlines()[-1]
    point = out.split(None, 5)[5]
    kinds = subprocess.run(["/sbin/mount"], capture_output=True, text=True, check=True).stdout
    m = re.search(r" on " + re.escape(point) + r" \((\w+)", kinds)
    return Path(point), (m.group(1).lower() if m else "")


def volume_label(path):
    point, _ = mount_of(path)
    return "this Mac" if str(point) in ("/", "/System/Volumes/Data") else point.name


def tree(path):
    """(files, bytes, links) under path, without following links."""
    files = size = links = 0
    for root, dirs, names in os.walk(path):
        # exFAT keeps extended attributes in "._name" side files; they are not game files.
        names = [n for n in names if not n.startswith("._")]
        for name in names + dirs:
            p = os.path.join(root, name)
            if os.path.islink(p):
                links += 1
            elif name in names:
                files += 1
                size += os.path.getsize(p)
    return files, size, links


def running(platform, folder_name):
    out = subprocess.run(["/bin/ps", "-axo", "command="], capture_output=True, text=True).stdout
    for line in out.splitlines():
        # Wine processes show a Windows command line; skip shells and editors that only mention the name.
        program = line.split(" ", 1)[0]
        if program.startswith("/") and "wine" not in program.lower():
            continue
        if CLIENT_EXE[platform] in line or ("\\" + folder_name + "\\") in line:
            return True
    return False


def game(game_id):
    platform = game_id.partition(":")[0]
    if platform not in BOTTLES:
        fail("Moving game files is available for Battle.net games.")
    listing = subprocess.run([sys.executable, str(ROOT / "scripts/game-library.py"), "list"], capture_output=True, text=True)
    for g in json.loads(listing.stdout or "{}").get("games", []):
        if g["id"] == game_id and g.get("install_path"):
            return platform, Path(g["install_path"])
    fail("This game is not installed.")


def games_root(platform):
    return BASE / BOTTLES[platform] / "drive_c/Program Files (x86)"


def inside_bottle(platform, path):
    root = games_root(platform)
    return path.parent == root or path.parent.resolve() == root.resolve()


def check_target(folder, need_bytes, has_links):
    folder = Path(folder).expanduser()
    if not folder.is_dir():
        fail("Choose a folder that exists on the drive.")
    point, kind = mount_of(folder)
    if kind not in FILESYSTEMS:
        fail(f"This drive's format ({kind or 'unknown'}) is not supported. Use APFS, Mac OS Extended or exFAT.")
    if kind == "exfat" and has_links:
        fail("This game contains links, which an exFAT drive cannot store. Use an APFS drive.")
    if not os.access(folder, os.W_OK):
        fail("Soju cannot write to that folder.")
    free = shutil.disk_usage(folder).free
    if free < need_bytes * 1.02 + 512 * 1024 * 1024:
        fail(f"Not enough space on {point.name}: {need_bytes / 1e9:.1f} GB needed, {free / 1e9:.1f} GB free.")
    return folder


def copy_verified(src, dst):
    dst.parent.mkdir(parents=True, exist_ok=True)
    print(f"Copying {src.name} ...", flush=True)
    done = subprocess.run(["/usr/bin/rsync", "-a", str(src) + "/", str(dst) + "/"])
    if done.returncode != 0:
        fail("Copying stopped before it finished. The game was left where it was.")
    print("Checking the copy ...", flush=True)
    a, b = tree(src), tree(dst)
    if a != b:
        fail(f"The copy does not match the original ({a[0]} files, {a[1]} bytes against {b[0]}, {b[1]}). The game was left where it was.")


def status(game_id):
    platform, path = game(game_id)
    real = path.resolve()
    connected = real.is_dir()
    parts = real.parts
    volume = volume_label(real) if connected else (parts[2] if len(parts) > 2 and parts[1] == "Volumes" else "another drive")
    return {"game": game_id, "path": str(path), "files": str(real), "moved": path.is_symlink(),
            "volume": volume, "connected": connected}


def move(game_id, folder):
    platform, path = game(game_id)
    if path.is_symlink():
        fail("This game is already on another drive. Move it back first.")
    if not inside_bottle(platform, path):
        fail("This game is not in Soju's Battle.net folder, so Soju leaves it where it is.")
    if running(platform, path.name):
        fail("Close Battle.net and the game first.")
    files, size, links = tree(path)
    folder = check_target(folder, size, links > 0)
    if mount_of(folder)[0] == mount_of(path)[0]:
        fail("That folder is on the same disk as the game.")
    dst = folder / "Soju Games" / platform / path.name
    if dst.exists() and any(dst.iterdir()):
        fail(f"{dst} already has files in it.")
    copy_verified(path, dst)
    (dst.parent / (path.name + MARK)).write_text(json.dumps({"game": game_id, "from": str(path)}) + "\n")
    aside = path.with_name(path.name + ".soju-moving")
    path.rename(aside)
    try:
        path.symlink_to(dst, target_is_directory=True)
        if not (path / ".").exists():
            raise OSError("link does not resolve")
    except OSError:
        if path.is_symlink():
            path.unlink()
        aside.rename(path)
        raise
    shutil.rmtree(aside)
    print(f"Moved to {dst} ({size / 1e9:.1f} GB). Keep the drive connected to play.", flush=True)
    return status(game_id)


def back(game_id):
    platform, path = game(game_id)
    if not path.is_symlink():
        fail("This game is already on this Mac.")
    real = path.resolve()
    if not real.is_dir():
        fail("Connect the drive that holds this game first.")
    if running(platform, path.name):
        fail("Close Battle.net and the game first.")
    files, size, links = tree(real)
    free = shutil.disk_usage(path.parent).free
    if free < size * 1.02 + 2 * 1024 ** 3:
        fail(f"Not enough space on this Mac: {size / 1e9:.1f} GB needed, {free / 1e9:.1f} GB free.")
    incoming = path.with_name(path.name + ".soju-moving")
    if incoming.exists():
        shutil.rmtree(incoming)
    copy_verified(real, incoming)
    path.unlink()
    incoming.rename(path)
    shutil.rmtree(real)
    (real.parent / (path.name + MARK)).unlink(missing_ok=True)
    print(f"Moved back to this Mac ({size / 1e9:.1f} GB).", flush=True)
    return status(game_id)


def place(platform, name, folder):
    if platform not in BOTTLES:
        fail("This is available for Battle.net games.")
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9 ._'-]{0,80}", name) or name.endswith((".", " ")):
        fail("Give the game's folder name as Battle.net shows it, for example: Diablo IV")
    root = games_root(platform)
    if not root.is_dir():
        fail("Install Battle.net first.")
    link = root / name
    if link.is_symlink() or link.exists():
        fail(f"{name} already has a folder in Battle.net's games folder.")
    if running(platform, name):
        fail("Close Battle.net first.")
    folder = check_target(folder, 0, False)
    dst = folder / "Soju Games" / platform / name
    dst.mkdir(parents=True, exist_ok=True)
    link.symlink_to(dst, target_is_directory=True)
    found = tree(dst)
    print(f"{name} will be kept in {dst}" + (f" ({found[0]} files already there)." if found[0] else "."), flush=True)
    print("In Battle.net, press Install and keep the default location.", flush=True)
    return {"link": str(link), "files": str(dst), "existing_files": found[0], "volume": volume_label(dst)}


def adopt(platform, source):
    """Use a game folder that already exists (another Wine app, a backup, an external drive)."""
    if platform not in BOTTLES:
        fail("This is available for Battle.net games.")
    source = Path(source).expanduser().resolve()
    if not source.is_dir():
        fail("Choose the game's folder.")
    if not (source / ".build.info").is_file():
        fail("That folder is not a Battle.net game: it has no .build.info file. Choose the folder that holds the game's .exe.")
    root = games_root(platform)
    if not root.is_dir():
        fail("Install Battle.net first.")
    if root.resolve() in source.parents:
        fail("That game is already in Soju's Battle.net folder.")
    _, kind = mount_of(source)
    if kind not in FILESYSTEMS:
        fail(f"This drive's format ({kind or 'unknown'}) is not supported. Use APFS, Mac OS Extended or exFAT.")
    if not os.access(source, os.W_OK):
        fail("Soju cannot write to that folder, and Battle.net needs to update the game there.")
    link = root / source.name
    if link.is_symlink() or link.exists():
        fail(f"{source.name} already has a folder in Battle.net's games folder.")
    if running(platform, source.name):
        fail("Close Battle.net first.")
    link.symlink_to(source, target_is_directory=True)
    print(f"Soju will use {source} without copying it.", flush=True)
    print(f"In Battle.net, open {source.name}, press Install (or Locate the game) and keep the default location.", flush=True)
    return {"link": str(link), "files": str(source), "volume": volume_label(source)}


def main():
    a = sys.argv[1:]
    if len(a) == 2 and a[0] == "status":
        print(json.dumps(status(a[1])))
    elif len(a) == 3 and a[0] == "move":
        print(json.dumps(move(a[1], a[2])))
    elif len(a) == 2 and a[0] == "back":
        print(json.dumps(back(a[1])))
    elif len(a) == 4 and a[0] == "place":
        print(json.dumps(place(a[1], a[2], a[3])))
    elif len(a) == 3 and a[0] == "import":
        print(json.dumps(adopt(a[1], a[2])))
    else:
        fail("usage: game-storage.py status|back <game id> | move <game id> <folder> | place battlenet <name> <folder> | import battlenet <game folder>")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, subprocess.SubprocessError, json.JSONDecodeError) as error:
        print("soju: " + str(error), file=sys.stderr)
        sys.exit(1)
