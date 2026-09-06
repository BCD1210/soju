#!/usr/bin/env python3
"""Guard Steam runtime changes and stop only the requested Wine prefix."""
import argparse
import os
from pathlib import Path
import subprocess
import sys


def server_paths():
    result = subprocess.run(["ps", "-axo", "comm="], check=True,
                            capture_output=True, text=True, timeout=10)
    return [Path(line.strip()) for line in result.stdout.splitlines()
            if Path(line.strip()).name in ("wineserver", "wineserver64")]


def is_steam_server(path, runtime):
    if path.resolve() == (runtime / "bin" / path.name).resolve():
        return True
    return any(part in ("Wine Stable.app", "steam-runtime", "steam-wine",
                        "steam-runtime.previous", "steam-wine.previous")
               or part.startswith(".steam-runtime.") for part in path.parts)


def require_idle(runtime):
    if any(is_steam_server(path, runtime) for path in server_paths()):
        raise RuntimeError("Close Windows Steam and its games before changing the Steam runtime.")


def stop(prefix, runtime, timeout=30):
    if not prefix.is_dir():
        return
    server = runtime / "bin/wineserver"
    if not os.access(server, os.X_OK):
        # A missing engine is harmless only when no Wine server is running.
        # Without its executable we cannot reliably stop or identify its prefix.
        if server_paths():
            raise RuntimeError("Cannot stop this bottle: its Wine runtime is missing. Close Wine launchers and games, then retry.")
        return
    env = {key: value for key, value in os.environ.items()
           if not key.startswith(("DYLD_", "WINE", "CX_", "ROSETTA_"))}
    env.update(WINEPREFIX=str(prefix.resolve()), WINEDEBUG="-all")
    for flag in ("-k", "-w"):
        subprocess.run([str(server), flag], env=env, check=True,
                       capture_output=True, text=True, timeout=timeout)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("idle", "stop"))
    parser.add_argument("runtime", type=Path)
    parser.add_argument("prefix", type=Path, nargs="?")
    args = parser.parse_args()
    try:
        if args.action == "idle":
            require_idle(args.runtime)
        else:
            if args.prefix is None:
                parser.error("stop requires a prefix")
            stop(args.prefix, args.runtime)
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        print("Steam runtime: " + str(error), file=sys.stderr)
        sys.exit(1)
