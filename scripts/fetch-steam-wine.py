#!/usr/bin/env python3
"""Install the pinned upstream Wine build privately; no Homebrew cask required."""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import sys
import tarfile
import tempfile

MANIFEST = Path(__file__).resolve().parents[1] / "resources/steam-wine.json"

def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()

def unpack(archive, dest, manifest):
    """Extract just Wine; links may only point to files within this runtime."""
    root = manifest["archive_root"] + "/"
    seen, links, total = set(), [], 0
    with tarfile.open(archive, "r:xz") as tar:
        for member in tar:
            raw = member.name.removeprefix("./")
            if PurePosixPath(raw).is_absolute() or ".." in PurePosixPath(raw).parts:
                raise ValueError("Unsafe Wine archive path")
            if not raw.startswith(root):
                continue
            name = raw[len(root):].rstrip("/")
            if not name:
                continue
            if name in seen:
                raise ValueError("Duplicate Wine archive entry")
            seen.add(name)
            target = dest / name
            target.parent.mkdir(parents=True, exist_ok=True)
            if member.isdir():
                target.mkdir(exist_ok=True)
            elif member.isfile():
                total += member.size
                if member.size > 256 * 1024 * 1024 or total > 2 * 1024**3:
                    raise ValueError("Oversized Wine archive")
                with tar.extractfile(member) as src, target.open("xb") as out:
                    shutil.copyfileobj(src, out)
                target.chmod(0o755 if member.mode & 0o111 else 0o644)
            elif member.issym():
                # Delay links until regular files are written, so archive writes
                # can never traverse an archive-supplied symlink.
                links.append((target, member.linkname))
            else:
                raise ValueError("Unsupported Wine archive entry")
    for target, link in links:
        if PurePosixPath(link).is_absolute():
            raise ValueError("Absolute Wine archive link")
        resolved = (target.parent / link).resolve()
        if dest.resolve() not in resolved.parents:
            raise ValueError("Wine archive link escapes runtime")
        target.symlink_to(link)
    for target, _ in links:
        resolved = target.resolve(strict=True)
        if dest.resolve() not in resolved.parents or not resolved.is_file():
            raise ValueError("Invalid Wine archive link")
    for name in ("bin/wine", "bin/wineserver", "lib/wine/x86_64-unix/ntdll.so",
                 "lib/wine/x86_64-windows/ntdll.dll", "share/wine/wine.inf"):
        if not (dest / name).is_file():
            raise ValueError("Incomplete Wine runtime")

def wine_version(target):
    env = {k: v for k, v in os.environ.items()
           if not k.startswith(("DYLD_", "WINE", "CX_", "ROSETTA_"))}
    return subprocess.run([str(target / "bin/wine"), "--version"], env=env,
                          check=True, capture_output=True, text=True, timeout=20).stdout.strip()

def ready(target, manifest):
    try:
        return ((target / ".soju-wine").read_text().strip() == manifest["sha256"]
                and os.access(target / "bin/wineserver", os.X_OK)
                and wine_version(target) == manifest["wine_version"])
    except (OSError, subprocess.SubprocessError):
        return False

def fetch(base, manifest, archive_path=None):
    target = base / "steam-wine"
    if ready(target, manifest):
        print("Steam Wine runtime ready (" + manifest["version"] + ")")
        return target
    base.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".steam-wine-", dir=base) as temp:
        work = Path(temp)
        archive = Path(archive_path) if archive_path else work / "wine.tar.xz"
        if not archive_path:
            print("Downloading Steam Wine runtime (" + manifest["version"] + ", 177 MB)…", flush=True)
            subprocess.run(["/usr/bin/curl", "--fail", "--location", "--proto", "=https",
                            "--proto-redir", "=https", "--retry", "3", "--connect-timeout", "20",
                            "--max-time", "900", manifest["url"], "--output", str(archive)], check=True)
        if digest(archive) != manifest["sha256"]:
            raise ValueError("Wine download checksum mismatch; existing runtime unchanged")
        stage = work / "runtime"
        stage.mkdir()
        unpack(archive, stage, manifest)
        if wine_version(stage) != manifest["wine_version"]:
            raise ValueError("Unexpected Wine version; existing runtime unchanged")
        (stage / ".soju-wine").write_text(manifest["sha256"] + "\n")
        backup = base / "steam-wine.previous"
        if backup.exists():
            shutil.rmtree(backup)
        if target.exists():
            target.rename(backup)
        try:
            stage.rename(target)
        except OSError:
            if backup.exists():
                backup.rename(target)
            raise
        if backup.exists():
            shutil.rmtree(backup)
    print("Steam Wine runtime installed in " + str(target))
    return target

if __name__ == "__main__":
    try:
        manifest = json.loads(MANIFEST.read_text())
        base = Path(os.environ.get("SOJU_BASE", str(Path.home() / ".battlenet-macos")))
        fetch(base, manifest)
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError, tarfile.TarError) as error:
        print("Steam Wine runtime: " + str(error), file=sys.stderr)
        sys.exit(1)
