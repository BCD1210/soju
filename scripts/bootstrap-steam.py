#!/usr/bin/env python3
"""Finish a new Steam client's update before installing its CEF wrapper."""
import os
from pathlib import Path
import subprocess
import sys
import time

def client_ready(steam):
    return ((steam / "steamui.dll").is_file() and
            any((steam / "bin/cef").glob("cef.win*/steamwebhelper.exe")))

def bootstrap(wine, prefix, timeout=900):
    steam = prefix / "drive_c/Program Files (x86)/Steam"
    if client_ready(steam):
        return
    env = {k: v for k, v in os.environ.items()
           if not k.startswith(("DYLD_", "WINE", "CX_", "ROSETTA_"))}
    env.update(WINEPREFIX=str(prefix), WINEDEBUG="-all")
    server = str(wine.parent / "wineserver")
    log = steam / "logs/bootstrap_log.txt"
    start = log.stat().st_size if log.exists() else 0
    print("==> Downloading the full Steam client before preparing its login screen…", flush=True)
    process = subprocess.Popen([str(wine), str(steam / "steam.exe"),
                                "-silent", "-shutdown"], env=env, stdin=subprocess.DEVNULL)
    complete = False
    try:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            text = ""
            if log.exists():
                with log.open("rb") as stream:
                    if log.stat().st_size >= start:
                        stream.seek(start)
                    text = stream.read()[-65536:].decode("utf-8", errors="replace")
            # The bootstrapper may exit with 42 and relaunch itself during an
            # update. Wait for its final verification, not that first process.
            if client_ready(steam) and verification_finished(text):
                complete = True
                break
            time.sleep(1)
    finally:
        # This branch only runs for an incomplete client, before any sign-in or
        # game launch. Stop this prefix alone so the wrapper can be installed.
        subprocess.run([server, "-k"], env=env, timeout=20, check=False)
        subprocess.run([server, "-w"], env=env, timeout=30, check=False)
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.terminate()
    if not complete:
        raise RuntimeError("Steam's first download did not finish. Check your connection and retry installation.")
    print("==> Steam client downloaded; preparing the login screen", flush=True)

def verification_finished(text):
    verified = text.rfind("Verification complete")
    pending = max(text.rfind("Startup - updater"), text.rfind("Downloading update"),
                  text.rfind("Extracting package"), text.rfind("Installing update"))
    return verified >= 0 and verified > pending

if __name__ == "__main__":
    try:
        bootstrap(Path(sys.argv[1]), Path(os.environ["WINEPREFIX"]))
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        print("Steam setup: " + str(error), file=sys.stderr)
        sys.exit(1)
