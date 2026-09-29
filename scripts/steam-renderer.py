#!/usr/bin/env python3
"""Select DXMT or DXVK for one 64-bit Steam game; preserve the Steam client."""
import argparse
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import struct
import subprocess
import sys
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]
DLLS = ("d3d11", "d3d10core", "dxgi", "winemetal")
FILES = ("d3d11.dll", "d3d10core.dll")
RESERVED = {"steam.exe", "steamwebhelper.exe", "steamwebhelper_real.exe",
            "steamservice.exe", "wineboot.exe", "explorer.exe", "services.exe"}
spec = importlib.util.spec_from_file_location("steam_session", ROOT / "scripts/steam-session.py")
session = importlib.util.module_from_spec(spec)
spec.loader.exec_module(session)


def signature(path):
    if path.is_symlink() or (path.exists() and not path.is_file()):
        raise ValueError("Refusing to replace a link or non-file: " + str(path))
    return hashlib.sha256(path.read_bytes()).hexdigest() if path.exists() else None


def atomic(path, data):
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as stream:
        temporary = Path(stream.name)
        try:
            stream.write(data)
            stream.flush()
            os.fchmod(stream.fileno(), path.stat().st_mode & 0o777 if path.exists() else 0o600)
            os.replace(temporary, path)
        finally:
            temporary.unlink(missing_ok=True)


def section(text, exe):
    key = ("Software\\Wine\\AppDefaults\\" + exe + "\\DllOverrides").replace("\\", "\\\\")
    lines = text.splitlines(keepends=True)
    start = end = len(lines)
    matches = []
    for i, line in enumerate(lines):
        if line.startswith("["):
            if start < len(lines) and end == len(lines):
                end = i
            if line.split("]", 1)[0][1:].casefold() == key.casefold():
                matches.append(i)
                start, end = i, len(lines)
    if len(matches) > 1:
        raise ValueError("Duplicate game registry sections; no changes made")
    return lines, start, end, key


def overrides(text, exe):
    lines, start, end, _ = section(text, exe)
    result = dict.fromkeys(DLLS)
    seen = set()
    for line in lines[start + 1:end]:
        match = re.match(r'^"([^"]+)"=(.*)', line)
        if match and match[1].casefold() in result:
            name = match[1].casefold()
            if name in seen:
                raise ValueError("Duplicate renderer override: " + name)
            seen.add(name)
            value = json.loads(match[2])
            if not isinstance(value, str):
                raise ValueError("Unsupported renderer registry value: " + name)
            result[name] = value
    return result


def with_overrides(text, exe, values):
    lines, start, end, key = section(text, exe)
    kept = [line for line in lines[start + 1:end]
            if not any(line.casefold().startswith('"' + dll + '"=') for dll in DLLS)]
    added = [json.dumps(k) + "=" + json.dumps(v, ensure_ascii=False) + "\n"
             for k, v in values.items() if v is not None]
    if start == len(lines):
        return text + ("\n[" + key + "]\n" + "".join(added) if added else "")
    return "".join(lines[:start + 1] + added + kept + lines[end:])


def validate_game(game):
    if game.name.casefold() in RESERVED or not re.fullmatch(r"[\w .()+-]+\.exe", game.name, re.I):
        raise ValueError("Choose the game's .exe, not a Steam client or system executable")
    with game.open("rb") as stream:
        if stream.read(2) != b"MZ":
            raise ValueError("The game must be a 64-bit Windows executable")
        stream.seek(0x3c)
        offset = struct.unpack("<I", stream.read(4))[0]
        stream.seek(offset)
        if stream.read(6) != b"PE\0\0\x64\x86":
            raise ValueError("Only x86-64 games are supported by this renderer switch")


def fetch_dxvk(base):
    manifest = json.loads((ROOT / "resources/steam-dxvk.json").read_text())
    target = base / "steam-support" / ("dxvk-" + manifest["version"])
    if all(signature(target / name) == sha for name, sha in manifest["files"].items()):
        return target
    target.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=target.parent) as work:
        work = Path(work)
        archive = work / "dxvk.tar.gz"
        subprocess.run(["/usr/bin/curl", "--fail", "--location", "--proto", "=https",
                        "--connect-timeout", "20", "--max-time", "180", "--retry", "2",
                        manifest["url"], "--output", str(archive)], check=True)
        if signature(archive) != manifest["sha256"]:
            raise ValueError("DXVK download checksum mismatch; game unchanged")
        wanted = {manifest["archive_root"] + "/x64/" + name: name for name in FILES}
        seen = set()
        with tarfile.open(archive, "r:gz") as tar:
            for member in tar:
                path = PurePosixPath(member.name)
                if path.is_absolute() or ".." in path.parts or not (member.isfile() or member.isdir()):
                    raise ValueError("Unsafe DXVK archive")
                if member.name not in wanted:
                    continue
                name = wanted[member.name]
                if name in seen or not member.isfile() or member.size > 32 * 1024 * 1024:
                    raise ValueError("Invalid DXVK archive member")
                seen.add(name)
                with tar.extractfile(member) as source:
                    (work / name).write_bytes(source.read())
        if seen != set(FILES) or any(signature(work / n) != sha for n, sha in manifest["files"].items()):
            raise ValueError("DXVK files failed checksum verification")
        target.mkdir(exist_ok=True)
        for name in FILES:
            atomic(target / name, (work / name).read_bytes())
    return target


def configure(base, prefix, runtime, game, mode):
    game = game.expanduser().resolve(strict=True)
    validate_game(game)
    registry = prefix / "user.reg"
    if not registry.is_file() or registry.is_symlink():
        raise ValueError("Install Steam first: soju steam-install")
    states = prefix / ".soju-renderers"
    record = states / (hashlib.sha256(game.name.casefold().encode()).hexdigest() + ".json")
    state = json.loads(record.read_text()) if record.exists() else None
    if state and state["game"] != str(game):
        raise ValueError("Another game with this executable name has a profile in this bottle. Reset it first.")
    if states.is_dir():
        for other in states.glob("*.json"):
            if other != record and Path(json.loads(other.read_text())["game"]).parent == game.parent:
                raise ValueError("Another executable in this directory owns the shared DLLs. Reset its profile first.")
    text = registry.read_text(encoding="utf-8", errors="surrogateescape")
    current = overrides(text, game.name)
    if mode == "status":
        return {"game": str(game), "renderer": state["renderer"] if state else "unmanaged",
                "overrides": current}
    session.require_idle(runtime)
    # Reset remains available if the runtime has been removed or damaged.
    if mode != "reset" and (not (runtime / ".soju-runtime").is_file() or
                            not (prefix / ".soju-steam-games").is_file()):
        raise ValueError("Prepare the Soju Steam renderer first: soju steam-games")
    if mode == "reset" and state is None:
        return {"game": str(game), "renderer": "unmanaged", "message": "No Soju renderer profile to reset"}
    paths = {name: game.parent / name for name in FILES}
    # A case variant would be the same DLL to Wine, but a second file on some Macs.
    for child in game.parent.iterdir():
        if child.name.casefold() in FILES and child.name != child.name.casefold():
            raise ValueError("Rename the existing DLL to lowercase before configuring it: " + str(child))
    signatures = {name: signature(path) for name, path in paths.items()}
    if state and (signatures != state["managed_files"] or current != state["managed_overrides"]):
        raise ValueError("Game DLLs or overrides changed outside Soju; preserving them. Inspect the renderer profile before retrying.")
    backup = states / record.stem
    if state:
        for name, sha in state["original_files"].items():
            if sha is not None and signature(backup / name) != sha:
                raise ValueError("Renderer recovery copy is missing or changed")
    payload = fetch_dxvk(base) if mode == "dxvk" else None
    # The download can take time. Recheck before touching the game or registry.
    session.require_idle(runtime)
    if registry.read_text(encoding="utf-8", errors="surrogateescape") != text or any(
            signature(paths[n]) != signatures[n] for n in FILES):
        raise ValueError("Steam settings or game files changed during preparation; retry after closing Steam")
    states.mkdir(exist_ok=True)
    if state is None:
        backup.mkdir(exist_ok=False)
        try:
            for name, path in paths.items():
                if signatures[name] is not None:
                    shutil.copy2(path, backup / name)
        except BaseException:
            shutil.rmtree(backup)
            raise
        state = {"game": str(game), "original_overrides": current, "original_files": signatures}
    original_state = record.read_bytes() if record.exists() else None
    before = {name: path.read_bytes() if path.exists() else None for name, path in paths.items()}
    desired = (dict.fromkeys(DLLS, "builtin") if mode == "dxmt" else
               {"d3d11": "native", "d3d10core": "native", "dxgi": "native", "winemetal": "disabled"}
               if mode == "dxvk" else state["original_overrides"])
    # Persist recovery information before modifying any game file.
    atomic(record, json.dumps({**state, "renderer": "recovery-required",
                              "managed_files": signatures, "managed_overrides": current}, indent=2).encode())
    try:
        for name, path in paths.items():
            data = ((payload / name).read_bytes() if payload else
                    (backup / name).read_bytes() if state["original_files"][name] is not None else None)
            if data is None:
                path.unlink(missing_ok=True)
            else:
                atomic(path, data)
        atomic(registry, with_overrides(text, game.name, desired).encode("utf-8", errors="surrogateescape"))
        if mode == "reset":
            record.unlink()
        else:
            state.update(renderer=mode, managed_files={n: signature(p) for n, p in paths.items()},
                         managed_overrides=desired)
            atomic(record, json.dumps(state, indent=2).encode())
    except BaseException:
        # Restore all original bytes on an interrupted/failed transaction.
        for name, path in paths.items():
            if before[name] is None:
                path.unlink(missing_ok=True)
            else:
                atomic(path, before[name])
        atomic(registry, text.encode("utf-8", errors="surrogateescape"))
        if original_state is None:
            record.unlink(missing_ok=True)
            shutil.rmtree(backup)
        else:
            atomic(record, original_state)
        raise
    if mode == "reset":
        shutil.rmtree(backup)
    return {"game": str(game), "renderer": "original" if mode == "reset" else mode,
            "message": "Restart Windows Steam and the game. Other executable profiles are unchanged."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("dxvk", "dxmt", "reset", "status"))
    parser.add_argument("game", type=Path, help="full Mac path to the game's 64-bit .exe")
    args = parser.parse_args()
    base = Path(os.environ.get("SOJU_BASE", str(Path.home() / ".battlenet-macos"))).expanduser().resolve()
    prefix = Path(os.environ.get("WINEPREFIX", str(base / "steam-bottle"))).expanduser().resolve()
    runtime = Path(os.environ.get("SOJU_STEAM_WINE", str(base / "steam-runtime"))).expanduser().resolve()
    if not prefix.is_dir():
        raise ValueError("Install Steam first: soju steam-install")
    if args.mode == "status":
        result = configure(base, prefix, runtime, args.game, args.mode)
    else:
        with (prefix / ".soju-renderer.lock").open("a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            result = configure(base, prefix, runtime, args.game, args.mode)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, RuntimeError, struct.error,
            subprocess.SubprocessError, tarfile.TarError) as error:
        print("Steam renderer: " + str(error), file=sys.stderr)
        sys.exit(1)
