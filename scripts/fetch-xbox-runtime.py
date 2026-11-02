#!/usr/bin/env python3
"""Fetch the Xbox game runtime DLLs (xgameruntime and friends) for the Xbox prefix.

GDK games need Xbox services inside Wine. Those come from the Xodus Nexus runtime
(Wine 11.0 + xgameruntime, LGPL-2.1-or-later). MACxus rebuilt five of its DLLs
for macOS Wine; they ship inside its signed release image, and libnghttp2.dll
comes from the Nexus runtime release. xgameruntime.dll is Soju's own build with
one more fix (scripts/build-xbox-runtime.sh), published as a Soju release file.
Every source is pinned by size and SHA-256 in resources/xbox-runtime.json, and
every DLL is checked again after extraction. The image is mounted read-only and
nothing from it is executed. DLLs already installed that still match their pin
are kept, so an update downloads only what changed.
"""
import hashlib
import json
import os
import signal
from pathlib import Path, PurePosixPath
import shutil
import struct
import subprocess
import sys
import tarfile
import tempfile

MANIFEST = Path(__file__).resolve().parents[1] / 'resources/xbox-runtime.json'
LIMIT = 32 * 1024 * 1024


def digest_bytes(data):
    return hashlib.sha256(data).hexdigest()


def digest(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def recipe(manifest):
    return digest_bytes(json.dumps(manifest, sort_keys=True).encode())


def ready(target, manifest):
    try:
        if (target / '.soju-xbox-runtime').read_text().strip() != recipe(manifest):
            return False
        return all(digest(target / name) == value for name, value in manifest['dlls'].items())
    except OSError:
        return False


def download(url, dest, size, sha256, label):
    print('Downloading ' + label + ' (' + str(size // 1000000) + ' MB)…', flush=True)
    subprocess.run(['/usr/bin/curl', '--fail', '--silent', '--show-error', '--location', '--proto', '=https',
                    '--proto-redir', '=https', '--retry', '3', '--connect-timeout', '20', '--max-time', '1800',
                    url, '--output', str(dest)], check=True)
    if dest.stat().st_size != size or digest(dest) != sha256:
        raise ValueError(label + ' checksum mismatch; nothing was changed')


def asar_files(path, folder):
    """Read files under one folder of an Electron asar archive (header + concatenated data)."""
    with Path(path).open('rb') as f:
        _, header_size, _, length = struct.unpack('<IIII', f.read(16))
        if length > 64 * 1024 * 1024:
            raise ValueError('Unexpected archive header')
        header = json.loads(f.read(length))
        node = header
        for part in folder.split('/'):
            node = node['files'][part]
        result = {}
        for name, item in node['files'].items():
            if 'files' in item or item.get('unpacked') or '/' in name:
                continue
            size, offset = int(item['size']), int(item['offset'])
            if size > LIMIT:
                raise ValueError('Unexpected file size in archive')
            f.seek(8 + header_size + offset)
            result[name] = f.read(size)
        return result


def from_image(image, manifest, stage):
    mount = Path(tempfile.mkdtemp(prefix='soju-xbox-image-'))
    try:
        subprocess.run(['/usr/bin/hdiutil', 'attach', '-readonly', '-nobrowse', '-noautoopen', '-noverify',
                        '-mountpoint', str(mount), str(image)], check=True, capture_output=True)
        try:
            files = asar_files(mount / manifest['macxus']['asar'], manifest['macxus']['folder'])
        finally:
            subprocess.run(['/usr/bin/hdiutil', 'detach', '-force', str(mount)], capture_output=True)
    finally:
        shutil.rmtree(mount, ignore_errors=True)
    for name, value in manifest['dlls'].items():
        if name in files and name not in soju_names(manifest):
            if digest_bytes(files[name]) != value:
                raise ValueError(name + ' does not match the pinned runtime; nothing was changed')
            (stage / name).write_bytes(files[name])


def soju_names(manifest):
    return set(manifest.get('soju', {}).get('dlls', []))


def from_runtime(archive, manifest, stage):
    member = manifest['nexus']['member']
    name = PurePosixPath(member).name
    with tarfile.open(archive, 'r:xz') as tar:
        for item in tar:
            if item.name == member and item.isfile() and item.size <= LIMIT:
                with tar.extractfile(item) as source:
                    data = source.read()
                if digest_bytes(data) != manifest['dlls'][name]:
                    raise ValueError(name + ' does not match the pinned runtime; nothing was changed')
                (stage / name).write_bytes(data)
                return
    raise ValueError(name + ' is missing from the runtime archive')


def fetch(base, manifest):
    target = base / 'xbox-runtime'
    if ready(target, manifest):
        print('Xbox game runtime ready (' + manifest['runtime'] + ')')
        return target
    (base / 'build').mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.xbox-runtime-', dir=base / 'build') as temp:
        work = Path(temp)
        stage = work / 'runtime'
        stage.mkdir()
        for name, value in manifest['dlls'].items():
            try:
                if (target / name).is_file() and digest(target / name) == value:
                    shutil.copyfile(target / name, stage / name)
            except OSError:
                pass
        def missing(names):
            return [n for n in names if not (stage / n).is_file()]
        nexus = PurePosixPath(manifest['nexus']['member']).name
        ours = soju_names(manifest)
        if missing(n for n in manifest['dlls'] if n not in ours and n != nexus):
            image = work / 'macxus.dmg'
            download(manifest['macxus']['url'], image, manifest['macxus']['size'], manifest['macxus']['sha256'], 'MACxus runtime DLLs')
            from_image(image, manifest, stage)
            image.unlink()
        if missing([nexus]):
            archive = work / 'runtime.tar.xz'
            download(manifest['nexus']['url'], archive, manifest['nexus']['size'], manifest['nexus']['sha256'], 'Xodus Nexus runtime')
            from_runtime(archive, manifest, stage)
            archive.unlink()
        for name in missing(sorted(ours)):
            download(manifest['soju']['url'], stage / name, manifest['soju']['size'], manifest['soju']['sha256'], 'Soju ' + name)
        missing = [n for n in manifest['dlls'] if not (stage / n).is_file()]
        if missing:
            raise ValueError('Runtime DLLs missing: ' + ', '.join(missing))
        (stage / '.soju-xbox-runtime').write_text(recipe(manifest) + '\n')
        backup = base / 'xbox-runtime.previous'
        shutil.rmtree(backup, ignore_errors=True)
        if target.exists():
            target.rename(backup)
        try:
            stage.rename(target)
        except OSError:
            if backup.exists():
                backup.rename(target)
            raise
        shutil.rmtree(backup, ignore_errors=True)
    print('Xbox game runtime installed in ' + str(target))
    return target


if __name__ == '__main__':
    # Stop from Soju (SIGTERM to the process group): exit normally so temporary
    # folders and partial downloads are cleaned up.
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(143))
    try:
        manifest = json.loads(MANIFEST.read_text())
        base = Path(os.environ.get('SOJU_BASE', str(Path.home() / '.battlenet-macos')))
        if sys.argv[1:] == ['--check']:
            sys.exit(0 if ready(base / 'xbox-runtime', manifest) else 1)
        fetch(base, manifest)
    except (OSError, ValueError, KeyError, struct.error, tarfile.TarError, subprocess.SubprocessError) as error:
        print('Xbox game runtime: ' + str(error), file=sys.stderr)
        sys.exit(1)
