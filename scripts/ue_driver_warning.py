#!/usr/bin/env python3
"""Unreal Engine games stop at start with "WARNING: Known issues with graphics driver" on a Mac.

Apple's toolkit presents the GPU as "AMD Compatibility Mode" with driver 30.0.15.1233, which Unreal's
driver list calls outdated (it recommends 23.2.1). The game waits behind that window until it is
closed, and a window that opens behind others looks like a game that never starts (Hogwarts Legacy,
2026-11-24). Unreal reads r.WarnOfBadDrivers from the game's own Engine.ini; setting it to 0 skips the
window and changes nothing else. (The driver version Unreal checks is not the one in the registry:
setting RadeonSoftwareVersion there did not help.)

  ue_driver_warning.py <bottle>
      For each Unreal game that has run in the bottle (it has
      users/<name>/AppData/Local/<Game>/Saved/Config/Windows[NoEditor]/), make sure its Engine.ini says
      r.WarnOfBadDrivers=0 under [SystemSettings]. A game that has never run gets it after its first start.
"""
import sys
from pathlib import Path

SKIP = {"epicgameslauncher", "crashreportclient", "unrealengine", "unrealenginelauncher"}
LINE = "r.WarnOfBadDrivers=0"


def quiet(ini):
    try:
        raw = ini.read_bytes() if ini.exists() else b""
    except OSError:
        return False
    text = raw.decode("utf-8-sig", "replace")
    if "r.warnofbaddrivers" in text.lower():
        return False
    nl = "\r\n" if "\r\n" in text or not text else "\n"
    lines = text.splitlines()
    for i, line in enumerate(lines):
        if line.strip().lower() == "[systemsettings]":
            lines.insert(i + 1, LINE)
            break
    else:
        if lines and lines[-1].strip():
            lines.append("")
        lines += ["[SystemSettings]", LINE]
    try:
        # open(), not Path.write_text(): write_text takes newline= only from Python 3.10, and the
        # python3 that ships with macOS is 3.9.
        with open(ini, "w", encoding="utf-8", newline="") as f:
            f.write(nl.join(lines) + nl)
        return True
    except OSError:
        return False


def main(argv):
    if len(argv) != 1:
        print("usage: ue_driver_warning.py <bottle>", file=sys.stderr)
        return 64
    users = Path(argv[0]) / "drive_c/users"
    changed = 0
    for config in users.glob("*/AppData/Local/*/Saved/Config"):
        if config.parent.parent.name.lower() in SKIP:
            continue
        for platform in ("WindowsNoEditor", "Windows"):
            if (config / platform).is_dir():
                changed += quiet(config / platform / "Engine.ini")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
