#!/usr/bin/env python3
"""Fetch pinned components. Verify before extraction or replacement."""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import sys
import tarfile
import tempfile

MANIFEST = Path(__file__).resolve().parent.parent / "resources/steam-support.json"

def digest(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""): h.update(block)
    return h.hexdigest()

def verified(directory, manifest):
    return all((directory / name).is_file() and not (directory / name).is_symlink()
               and digest(directory / name) == sha for name, sha in manifest["files"].items())

def unpack(archive, dest, manifest):
    """Only regular allowlisted files; reject links, duplicates and traversal."""
    seen = set()
    with tarfile.open(archive, "r:gz") as tar:
        for member in tar:
            name = member.name[2:] if member.name.startswith("./") else member.name
            path = PurePosixPath(name)
            if path.is_absolute() or ".." in path.parts:
                raise ValueError("Unsafe archive path")
            if member.isdir(): continue
            if not member.isfile() or name not in manifest["files"] or name in seen:
                raise ValueError("Unexpected archive entry: " + name)
            if member.size > 150 * 1024 * 1024:
                raise ValueError("Oversized archive entry")
            seen.add(name)
            target = dest / name
            target.parent.mkdir(parents=True, exist_ok=True)
            with tar.extractfile(member) as src, target.open("wb") as out:
                shutil.copyfileobj(src, out)
    if seen != set(manifest["files"]) or not verified(dest, manifest):
        raise ValueError("Incomplete or corrupt Steam components")

def fetch(base, manifest):
    support = base / "steam-support"
    target = support / "prebuilt"
    if verified(target, manifest):
        print("Steam components verified (" + manifest["version"] + ")")
        return target
    support.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".download-", dir=support) as work:
        work = Path(work); archive = work / "support.tar.gz"; stage = work / "payload"
        stage.mkdir()
        print("Downloading Steam components: " + manifest["version"], flush=True)
        subprocess.run(["/usr/bin/curl", "--fail", "--location", "--proto", "=https",
                        "--retry", "3", "--connect-timeout", "20", "--max-time", "900",
                        manifest["url"], "--output", str(archive)], check=True)
        if digest(archive) != manifest["sha256"]:
            raise ValueError("Download checksum mismatch; existing files unchanged")
        unpack(archive, stage, manifest)
        (stage / ".soju-support-version").write_text(manifest["version"] + "\n")
        backup = support / "prebuilt.previous"
        if backup.exists(): shutil.rmtree(backup)
        if target.exists(): target.rename(backup)
        try: stage.rename(target)
        except OSError:
            if backup.exists(): backup.rename(target)
            raise
        if backup.exists(): shutil.rmtree(backup)
    return target

if __name__ == "__main__":
    try:
        manifest = json.loads(MANIFEST.read_text())
        base = Path(os.environ.get("SOJU_BASE", str(Path.home() / ".battlenet-macos")))
        if sys.argv[1:] == ["--check"]:
            sys.exit(0 if verified(base / "steam-support/prebuilt", manifest) else 1)
        fetch(base, manifest)
    except (OSError, ValueError, subprocess.CalledProcessError, tarfile.TarError) as e:
        print("Steam components: " + str(e), file=sys.stderr)
        sys.exit(1)
