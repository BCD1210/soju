#!/usr/bin/env python3
"""Install the part of GStreamer that Wine needs to decode game audio.

Wine's winegstreamer decodes Windows Media Audio and other compressed audio with
GStreamer. Neither Wine engine Soju ships includes GStreamer, so games whose voice
lines are WMA (Skyrim Special Edition, and other XAudio2/xWMA games) play music
and effects but no dialogue: "GStreamer doesn't support WMA decoding" (#94).

This downloads the official GStreamer macOS package from gstreamer.freedesktop.org
(pinned by SHA-256), keeps only the x86_64 parts of the plugins Wine uses for audio
(app, audioconvert, audioresample, coreelements, libav and the playback and
container plugins) plus the libraries they load, and deletes the rest. Nothing is
redistributed by Soju; the files come from GStreamer's own release.

  fetch-gstreamer.py          install or verify   -> prints the install folder
  fetch-gstreamer.py --check  exit 0 when installed
"""
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

VERSION = "1.28.9"
URL = f"https://gstreamer.freedesktop.org/data/pkg/osx/{VERSION}/gstreamer-1.0-{VERSION}-universal.pkg"
SHA256 = "05d27c8e9b4374bbcf1788cf1604bb4b1a4bdcbdc13bc37b67d3a225c8aa86a6"
PLUGINS = ["coreelements", "app", "audioconvert", "audioresample", "audioparsers", "libav",
           "playback", "typefindfunctions", "asf", "wavparse", "isomp4", "ogg", "vorbis", "opus"]
BASE = Path(os.environ.get("SOJU_BASE", str(Path.home() / ".battlenet-macos")))
DEST = BASE / "gstreamer"
MARK = DEST / ".soju-gstreamer"


def installed():
    return MARK.is_file() and MARK.read_text().strip() == VERSION and (DEST / "lib/gstreamer-1.0/libgstlibav.dylib").is_file()


def deps(path):
    """@rpath libraries a Mach-O file links, x86_64 slice."""
    out = subprocess.run(["/usr/bin/otool", "-arch", "x86_64", "-L", str(path)],
                         capture_output=True, text=True, check=True).stdout
    # otool also lists the file's own install name (@rpath/<its name>); that is not a dependency.
    return [m.group(1) for m in re.finditer(r"^\s+@rpath/(\S+)", out, re.M) if m.group(1) != Path(path).name]


def thin_copy(src, dst):
    dst.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(["/usr/bin/lipo", str(src), "-thin", "x86_64", "-output", str(dst)], check=True,
                   capture_output=True)
    dst.chmod(0o755)


def install():
    with tempfile.TemporaryDirectory(prefix="soju-gstreamer-") as tmp:
        tmp = Path(tmp)
        pkg = tmp / "gstreamer.pkg"
        print(f"Downloading GStreamer {VERSION} (about 150 MB, once) for game audio", flush=True)
        subprocess.run(["/usr/bin/curl", "--fail", "--location", "--proto", "=https", "--retry", "3",
                        "--silent", "--show-error", URL, "--output", str(pkg)], check=True)
        got = subprocess.run(["/usr/bin/shasum", "-a", "256", str(pkg)], capture_output=True, text=True,
                             check=True).stdout.split()[0]
        if got != SHA256:
            raise RuntimeError(f"GStreamer download checksum mismatch: {got}")
        subprocess.run(["/usr/sbin/pkgutil", "--expand-full", str(pkg), str(tmp / "x")], check=True,
                       capture_output=True)
        root = tmp / "root"
        for part in sorted((tmp / "x").glob("*.pkg")):
            if (part / "Payload").is_dir():
                subprocess.run(["/usr/bin/ditto", str(part / "Payload"), str(root)], check=True)
        lib = root / "lib"
        stage = BASE / "gstreamer.new"
        shutil.rmtree(stage, ignore_errors=True)
        want, seen = [], set()
        for name in PLUGINS:
            plugin = lib / "gstreamer-1.0" / f"libgst{name}.dylib"
            if not plugin.is_file():
                raise RuntimeError(f"GStreamer plugin missing from the package: {name}")
            thin_copy(plugin, stage / "lib/gstreamer-1.0" / plugin.name)
            want += deps(plugin)
        while want:   # the libraries the plugins load, and theirs
            name = want.pop()
            if name in seen:
                continue
            seen.add(name)
            src = lib / name
            if not src.is_file():
                raise RuntimeError(f"GStreamer library missing from the package: {name}")
            thin_copy(src, stage / "lib" / name)
            want += deps(src)
        scanner = root / "libexec/gstreamer-1.0/gst-plugin-scanner"
        thin_copy(scanner, stage / "libexec/gstreamer-1.0/gst-plugin-scanner")
        want = deps(scanner)
        while want:
            name = want.pop()
            if name not in seen:
                seen.add(name)
                thin_copy(lib / name, stage / "lib" / name)
                want += deps(lib / name)
        for license_dir in ("share/licenses", "share/doc"):
            if (root / license_dir).is_dir():
                shutil.copytree(root / license_dir, stage / license_dir, dirs_exist_ok=True)
        (stage / ".soju-gstreamer").write_text(VERSION + "\n")
        shutil.rmtree(DEST, ignore_errors=True)
        stage.rename(DEST)


def main():
    if "--check" in sys.argv:
        sys.exit(0 if installed() else 1)
    if not installed():
        install()
    print(DEST)


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print("GStreamer: " + str(error), file=sys.stderr)
        sys.exit(1)
