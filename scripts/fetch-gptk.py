#!/usr/bin/env python3
"""Download Apple's Game Porting Toolkit payload and install it with get-gptk.sh.

Apple's licence for the toolkit (section 2) lets the files under its /redist directory and
D3DMetal.framework be distributed separately, for non-commercial purposes. Soju does not
host them: it downloads the archive pinned in
resources/gptk.json, checks its SHA-256, takes only Apple's own files out of it and passes
them to get-gptk.sh, which installs nothing that is not signed by Apple (the Mach-O parts)
or a known Apple build (the PE shims).

    scripts/fetch-gptk.py            download and install into $ENGINE
    scripts/fetch-gptk.py --check    exit 0 when $ENGINE already has every shim of the build this Mac uses
    SOJU_GPTK_DOWNLOAD=0             callers skip this and ask for a dmg instead

Macs with an M6 or newer chip get the build under "newer_chips" (4.0 beta 3): D3DMetal 3.0
does not start Diablo II: Resurrected there.
"""
import hashlib
import json
import os
import re
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import sys
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "resources/gptk.json"

def digest(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""): h.update(block)
    return h.hexdigest()

def chip_generation(brand=None):
    """6 for "Apple M6 Pro", 0 when unknown (Intel, or sysctl missing)."""
    if brand is None:
        try:
            brand = subprocess.run(["/usr/sbin/sysctl", "-n", "machdep.cpu.brand_string"],
                                   capture_output=True, text=True).stdout
        except OSError:
            brand = ""
    m = re.search(r"\bApple M(\d+)\b", brand or "")
    return int(m.group(1)) if m else 0

def choose(manifest, generation):
    """The build for this Mac: the newer one on chips it names, else the default."""
    newer = manifest.get("newer_chips")
    if newer and generation >= newer["min_chip"]:
        return newer
    return manifest

def installed(engine, build):
    """Every shim of the build is in the engine and is one of Apple's (they all name D3DMetalDLLsBase)."""
    pe = Path(engine) / "lib/wine/x86_64-windows"
    if not (Path(engine) / "lib/external/libd3dshared.dylib").is_file():
        return False
    for shim in build["shims"]:
        try:
            if b"D3DMetalDLLsBase" not in (pe / shim).read_bytes():
                return False
        except OSError:
            return False
    return True

def wanted(name, manifest):
    """Path under the stage for an archive member Soju takes, else None."""
    prefix = manifest["prefix"].rstrip("/") + "/"
    if not name.startswith(prefix): return None
    rel = name[len(prefix):]
    if rel == "external" or rel.startswith("external/"): return rel
    if rel in {"wine/x86_64-windows/" + shim for shim in manifest["shims"]}: return rel
    return None

def unpack(archive, stage, manifest):
    """Apple's files only. Links are allowed inside the framework and must stay inside it."""
    with tarfile.open(archive, "r:xz") as tar:
        for member in tar:
            name = member.name[2:] if member.name.startswith("./") else member.name
            path = PurePosixPath(name)
            if path.is_absolute() or ".." in path.parts:
                raise ValueError("Unsafe archive path")
            rel = wanted(name.rstrip("/"), manifest)
            if rel is None: continue
            target = stage / rel
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True); continue
            if target.exists() or target.is_symlink():
                raise ValueError("Duplicate archive entry: " + name)
            target.parent.mkdir(parents=True, exist_ok=True)
            if member.issym():
                inside = "external/D3DMetal.framework/"
                resolved = os.path.normpath(os.path.join(os.path.dirname(rel), member.linkname))
                if (os.path.isabs(member.linkname) or not rel.startswith(inside)
                        or not (resolved + "/").startswith(inside)):
                    raise ValueError("Unsafe link in archive: " + name)
                os.symlink(member.linkname, target); continue
            if not member.isfile():
                raise ValueError("Unexpected archive entry: " + name)
            if member.size > 300 * 1024 * 1024:
                raise ValueError("Oversized archive entry")
            with tar.extractfile(member) as src, target.open("wb") as out:
                shutil.copyfileobj(src, out)
            target.chmod(0o755 if member.mode & 0o100 else 0o644)
    if not (stage / "external/libd3dshared.dylib").is_file() or not (stage / "external/D3DMetal.framework").is_dir():
        raise ValueError("The archive does not hold the toolkit payload")
    for shim in ("d3d11.dll", "d3d12.dll", "dxgi.dll"):
        if not (stage / "wine/x86_64-windows" / shim).is_file():
            raise ValueError("The archive is missing " + shim)

def main():
    manifest = choose(json.loads(MANIFEST.read_text()), chip_generation())
    base = Path(os.environ.get("SOJU_BASE", Path.home() / ".battlenet-macos"))
    engine = Path(os.environ.get("ENGINE", base / "cx26-engine"))
    if not (engine / "lib").is_dir():
        print("Engine not found at %s. Install Battle.net, Epic or GOG first; that prepares the engine." % engine)
        return 1
    if "--check" in sys.argv[1:]:
        return 0 if installed(engine, manifest) else 1
    print("Downloading Apple's Game Porting Toolkit %s (%d MB)" % (manifest["version"], manifest["size"] // 1000000), flush=True)
    base.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".gptk-download-", dir=base) as work:
        work = Path(work); archive = work / "gptk.tar.xz"; stage = work / "lib"
        stage.mkdir()
        # SOJU_GPTK_ARCHIVE: an archive already on disk (same SHA-256 check), for offline installs and tests.
        local = os.environ.get("SOJU_GPTK_ARCHIVE")
        if local:
            archive = Path(local)
        else:
            curl = ["/usr/bin/curl", "--fail", "--location", "--proto", "=https", "--retry", "3",
                    "--connect-timeout", "20", "--max-time", "1800", manifest["url"], "--output", str(archive)]
            curl.insert(1, "--progress-bar" if sys.stdout.isatty() else "--silent")
            if subprocess.run(curl).returncode != 0:
                print("  The download failed."); return 1
        if archive.stat().st_size != manifest["size"] or digest(archive) != manifest["sha256"]:
            print("  The downloaded archive does not match the pinned SHA-256; not using it."); return 1
        try:
            unpack(archive, stage, manifest)
        except (ValueError, tarfile.TarError, OSError) as error:
            print("  Could not read the archive: %s" % error); return 1
        if not local: archive.unlink()
        env = dict(os.environ, ENGINE=str(engine))
        if subprocess.run(["/bin/bash", str(ROOT / "scripts/get-gptk.sh"), str(stage)], env=env).returncode != 0:
            return 1
    return 0

if __name__ == "__main__":
    sys.exit(main())
