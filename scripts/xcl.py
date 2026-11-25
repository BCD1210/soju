#!/usr/bin/env python3
"""Xbox Series X|S game code on the Mac through XCL (experimental).

  soju xcl setup                    download XCL and Microsoft's shader compiler, make its bottle
  soju xcl run <game folder | .exe> [-- game arguments]
  soju xcl inspect|disasm|shaders|rpf ...   XCL's own tools (see soju xcl --help)
  soju xcl status                   what is installed (JSON with --json)
  soju xcl remove                   delete XCL and its bottle (--yes)

XCL (github.com/xcl-dev/xcl) loads unpacked Xbox Series executables and stands in for the
console's system libraries; MacBox (github.com/aykhanium/MacBox) ports it to Apple Silicon
with CrossOver. Here it runs on Soju's own engine and D3DMetal instead, in a bottle of its own.

No game is playable yet. No game files are included: XCL needs your own unpacked copy.
Checked 2026-11-25 on an M4 Pro, macOS 26.5: XCL starts, and its shader, compute and mesh
tests pass; its graphics draw test crashes inside Apple's D3DMetal after the draw is
submitted, with D3DMetal 3.0 and 4.0 beta 3 alike (MacBox reports it passing on macOS 27.2).
"""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import sys
import tarfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / 'resources/xcl.json'
BASE = Path(os.environ.get('SOJU_BASE', Path.home() / '.battlenet-macos'))
ENGINE = Path(os.environ.get('XCL_ENGINE', os.environ.get('ENGINE', BASE / 'cx26-engine')))
XCL = BASE / 'xcl'
BOTTLE = XCL / 'bottle'
RUNTIME = XCL / 'runtime'
TOOLS = ('inspect', 'disasm', 'shaders', 'rpf', '--help', '-h', '--version')


class XCLError(Exception):
    pass


def say(text):
    print(text, flush=True)


def digest(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def manifest():
    return json.loads(MANIFEST.read_text())


def recipe(m):
    return hashlib.sha256(json.dumps(m, sort_keys=True).encode()).hexdigest()


def fetch(entry, name, label):
    cache = XCL / 'downloads'
    cache.mkdir(parents=True, exist_ok=True)
    dest = cache / name
    if dest.is_file() and dest.stat().st_size == entry['size'] and digest(dest) == entry['sha256']:
        return dest
    say('Downloading ' + label + ' (' + str(max(1, entry['size'] // 1000000)) + ' MB)…')
    part = dest.with_suffix(dest.suffix + '.part')
    got = subprocess.run(['/usr/bin/curl', '--fail', '--silent', '--show-error', '--location', '--proto', '=https',
                          '--proto-redir', '=https', '--retry', '3', '--connect-timeout', '20', '--max-time', '1800',
                          entry['url'], '--output', str(part)], capture_output=True, text=True)
    if got.returncode:
        part.unlink(missing_ok=True)
        raise XCLError('Could not download ' + label + ': ' + (got.stderr.strip() or 'curl failed'))
    if part.stat().st_size != entry['size'] or digest(part) != entry['sha256']:
        part.unlink(missing_ok=True)
        raise XCLError(label + ' did not match its pinned checksum; nothing was installed.')
    part.replace(dest)
    return dest


def extract_bundle(archive, target):
    with tarfile.open(archive) as t:
        for member in t.getmembers():
            parts = PurePosixPath(member.name).parts
            if (not parts or parts[0] != 'soju-xcl' or '..' in parts or member.name.startswith('/')
                    or not (member.isfile() or member.isdir())):
                raise XCLError('Unexpected entry in the XCL bundle: ' + member.name)
        tmp = target.parent / (target.name + '.new')
        shutil.rmtree(tmp, ignore_errors=True)
        tmp.mkdir(parents=True)
        if hasattr(tarfile, 'data_filter'):
            t.extractall(tmp, filter='data')
        else:   # macOS's /usr/bin/python3 3.9: members were checked above
            t.extractall(tmp)
    shutil.rmtree(target, ignore_errors=True)
    (tmp / 'soju-xcl').rename(target)
    shutil.rmtree(tmp, ignore_errors=True)


def extract_dxc(archive, target, members):
    """The x64 DLLs and licenses of Microsoft's DXC release. Its zip names use backslashes."""
    with zipfile.ZipFile(archive) as z:
        for name in members:
            data = z.read(name)
            (target / name.replace('\\', '/').split('/')[-1]).write_bytes(data)


def winpath(path):
    return 'Z:' + str(Path(path).resolve()).replace('/', '\\')


def wine_env():
    env = dict(os.environ)
    env.update({
        'WINEPREFIX': str(BOTTLE),
        'WINEDEBUG': os.environ.get('SOJU_XCL_WINEDEBUG', '-all'),
        'WINEMSYNC': '1',
        'ROSETTA_ADVERTISE_AVX': '1',
        'WINE_SIMULATE_WRITECOPY': '1',
        'CX_ACTIVE_GRAPHICS_BACKEND': 'd3dmetal',
        'CX_GRAPHICS_BACKEND': 'd3dmetal',
        'CX_APPLEGPTK_LIBD3DSHARED_PATH': str(ENGINE / 'lib/external/libd3dshared.dylib'),
        'DYLD_FALLBACK_LIBRARY_PATH': str(ENGINE / 'lib') + ':/usr/lib',
        'XCL_DXC_DIR': winpath(RUNTIME),
    })
    return env


def status():
    state = {}
    try:
        state = json.loads((XCL / 'state.json').read_text())
    except (OSError, ValueError):
        pass
    return {
        'installed': (RUNTIME / 'xcl.exe').is_file() and (BOTTLE / 'system.reg').is_file(),
        'current': state.get('recipe') == recipe(manifest()),
        'engine_ready': (ENGINE / 'bin/wine').is_file() and (ENGINE / 'lib/external/libd3dshared.dylib').is_file(),
        'source': manifest()['bundle']['source_url'],
        'path': str(XCL),
    }


def setup():
    m = manifest()
    if not (ENGINE / 'bin/wine').is_file():
        raise XCLError("Soju's engine is not installed. Run: soju install")
    if not (ENGINE / 'lib/external/libd3dshared.dylib').is_file():
        raise XCLError("XCL needs Apple's D3DMetal. Run: soju gptk")
    bundle = fetch(m['bundle'], PurePosixPath(m['bundle']['url']).name, 'XCL (GPL-2.0, built from MacBox)')
    dxc = fetch(m['dxc'], 'dxc.zip', "Microsoft's shader compiler (DXC)")
    extract_bundle(bundle, RUNTIME)
    extract_dxc(dxc, RUNTIME, m['dxc']['members'])
    if not (BOTTLE / 'system.reg').is_file():
        say('Making the XCL bottle…')
        env = wine_env()
        subprocess.run([str(ENGINE / 'bin/wine'), 'wineboot', '-u'], env=env, capture_output=True)
        subprocess.run([str(ENGINE / 'bin/wineserver'), '-w'], env=env, capture_output=True)
    (XCL / 'state.json').write_text(json.dumps({'recipe': recipe(m)}, indent=2) + '\n')
    say('XCL is set up (experimental: no game is playable yet). Run a game folder with: soju xcl run <folder>')
    return 0


def xcl_args(args):
    """soju xcl run <path> [-- args] -> xcl.exe run Z:\\path [-- args]; the tools pass through,
    with existing Mac paths turned into Windows ones."""
    if not args:
        raise XCLError('Usage: soju xcl run <game folder | file.exe> [-- game arguments]')
    out, passthrough = [], False
    for a in args:
        if a == '--':
            passthrough = True
        elif not passthrough and a.startswith('/') and Path(a).exists():
            a = winpath(a)
        out.append(a)
    if out[0] == 'run' and len(out) < 2:
        raise XCLError('Choose a game folder or executable: soju xcl run <path>')
    return out


def run(args):
    if not status()['installed']:
        raise XCLError('XCL is not set up. Run: soju xcl setup')
    if args and args[0] == 'run' and len(args) > 1 and not Path(args[1]).exists():
        raise XCLError('Not found: ' + args[1])
    cmd = [str(ENGINE / 'bin/wine'), str(RUNTIME / 'xcl.exe')] + xcl_args(args)
    os.execve(cmd[0], cmd, wine_env())


def remove(yes):
    if not XCL.exists():
        say('XCL is not set up.')
        return 0
    if not yes:
        raise XCLError('This deletes ' + str(XCL) + ' (XCL saves in its bottle included). Run again with --yes.')
    subprocess.run([str(ENGINE / 'bin/wineserver'), '-k'], env=wine_env(), capture_output=True)
    shutil.rmtree(XCL)
    say('Removed ' + str(XCL) + '.')
    return 0


def main(argv):
    try:
        if not argv or argv[0] in ('help',):
            say(__doc__.strip())
            return 0
        cmd = argv[0]
        if cmd == 'setup':
            return setup()
        if cmd == 'status':
            info = status()
            say(json.dumps(info) if '--json' in argv else '\n'.join(k + ': ' + str(v) for k, v in info.items()))
            return 0
        if cmd == 'remove':
            return remove('--yes' in argv)
        if cmd == 'run' or cmd in TOOLS:
            return run(argv)
        raise XCLError('Unknown command: ' + cmd + '. See: soju xcl help')
    except XCLError as e:
        print('soju xcl: ' + str(e), file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
