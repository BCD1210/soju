#!/usr/bin/env python3
"""Diablo II: Resurrected in VR (offline): set up and play D2R VR Unleashed.

  soju vr setup [--refresh]   copy the Battle.net bottle and install the mod there
  soju vr play                start D2R in VR (offline characters only)
  soju vr status              what is installed (JSON with --json)
  soju vr remove              delete the VR copy (your Battle.net bottle is untouched)

The mod goes into an APFS copy of the Battle.net bottle (d2r-vr/bottle): the
copy takes almost no extra disk space at first, and the bottle Battle.net uses
is never modified. D2RLoader starts D2R with -mod, which keeps it offline.

Every download is pinned by size and SHA-256 in resources/d2r-vr.json. The VR
runtime (OXRSys + the wineopenxr bridge) is Soju's own build of pinned sources
(scripts/build-vr-runtime.sh). The mod's own installer runs under Windows
Python inside the copy, with the dependencies passed in already checked.

Three things the mod needs on Soju's engine, all inside the copy:
- the game folder's d3d12.dll is renamed, so D3D12 loads from system32, where
  ReShade watches for it (Wine already uses its builtin d3d12 either way);
- the windowed resolution is fixed at 1920x1080, the size the game first creates
  its swapchain at, so it never resizes it;
- one jump in ReShade64.dll skips work on a resource D3DMetal already destroyed
  (resources/d2r-vr.json, reshade.patch).
"""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import sys
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / 'resources/d2r-vr.json'
BASE = Path(os.environ.get('SOJU_BASE', Path.home() / '.battlenet-macos'))
ENGINE = Path(os.environ.get('ENGINE', BASE / 'cx26-engine'))
SOURCE = BASE / 'bottle'
VR = BASE / 'd2r-vr'
BOTTLE = VR / 'bottle'
GAME_REL = 'drive_c/Program Files (x86)/Diablo II Resurrected'
SAVES_REL = 'drive_c/users/crossover/Saved Games/Diablo II Resurrected'
STATE = VR / 'state.json'
LIMIT = 256 * 1024 * 1024


class VRError(Exception):
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
    """Download entry['url'] into the cache unless a checked copy is there."""
    cache = VR / 'downloads'
    cache.mkdir(parents=True, exist_ok=True)
    dest = cache / name
    if dest.is_file() and dest.stat().st_size == entry['size'] and digest(dest) == entry['sha256']:
        return dest
    if not entry.get('sha256') or not entry.get('size'):
        raise VRError(label + ' is not published yet for this Soju version.')
    say('Downloading ' + label + ' (' + str(max(1, entry['size'] // 1000000)) + ' MB)…')
    part = dest.with_suffix(dest.suffix + '.part')
    subprocess.run(['/usr/bin/curl', '--fail', '--silent', '--show-error', '--location', '--proto', '=https',
                    '--proto-redir', '=https', '--retry', '3', '--connect-timeout', '20', '--max-time', '1800',
                    entry['url'], '--output', str(part)], check=True)
    if part.stat().st_size != entry['size'] or digest(part) != entry['sha256']:
        part.unlink(missing_ok=True)
        raise VRError(label + ' did not match its pinned checksum; nothing was installed.')
    part.replace(dest)
    return dest


def safe_extract_zip(archive, target):
    target.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(archive) as z:
        for info in z.infolist():
            path = PurePosixPath(info.filename.replace('\\', '/'))
            if path.is_absolute() or '..' in path.parts:
                raise VRError('Unsafe path in ' + archive.name + ': ' + info.filename)
            out = target.joinpath(*path.parts)
            if info.is_dir():
                out.mkdir(parents=True, exist_ok=True)
                continue
            out.parent.mkdir(parents=True, exist_ok=True)
            with z.open(info) as src, out.open('wb') as dst:
                shutil.copyfileobj(src, dst)


def safe_extract_tar(archive, target):
    import tarfile
    target.mkdir(parents=True, exist_ok=True)
    with tarfile.open(archive) as t:
        for member in t.getmembers():
            path = PurePosixPath(member.name)
            if path.is_absolute() or '..' in path.parts or not (member.isfile() or member.isdir()):
                raise VRError('Unexpected entry in the VR runtime: ' + member.name)
        if hasattr(tarfile, 'data_filter'):
            t.extractall(target, filter='data')
        else:   # macOS's /usr/bin/python3 3.9: members were checked above
            t.extractall(target)


def winpath(path):
    return 'Z:' + str(Path(path).resolve()).replace('/', '\\')


def wine_env(prefix):
    env = dict(os.environ)
    env.update({
        'WINEPREFIX': str(prefix),
        'WINEDEBUG': os.environ.get('SOJU_VR_WINEDEBUG', 'fixme-all'),
        'WINEMSYNC': '1',
        'ROSETTA_ADVERTISE_AVX': '1',
        'WINE_SIMULATE_WRITECOPY': '1',
        'CX_ACTIVE_GRAPHICS_BACKEND': 'd3dmetal',
        'CX_GRAPHICS_BACKEND': 'd3dmetal',
        'CX_APPLEGPTK_LIBD3DSHARED_PATH': str(ENGINE / 'lib/external/libd3dshared.dylib'),
        'DYLD_FALLBACK_LIBRARY_PATH': str(ENGINE / 'lib') + ':/usr/lib',
    })
    env.pop('XR_RUNTIME_JSON', None)
    return env


def wine(prefix, args, **kwargs):
    return subprocess.run([str(ENGINE / 'bin/wine')] + args, env=wine_env(prefix), **kwargs)


def game_build(bottle):
    info = bottle / GAME_REL / '.build.info'
    try:
        text = info.read_text(errors='replace')
    except OSError:
        return None
    for line in reversed(text.splitlines()[1:]):
        for field in line.split('|'):
            parts = field.split('.')
            if len(parts) == 3 and all(p.isdigit() for p in parts):
                return field
    return None


SHELLS = {'bash', 'zsh', '-zsh', 'sh', 'grep', 'pgrep', 'ps', 'python3', 'python', 'osascript'}


def running(names):
    """Wine processes whose command line names one of these executables.

    A shell or script that only mentions the name (pgrep -f would match it)
    does not count."""
    out = subprocess.run(['/bin/ps', '-axo', 'args='], capture_output=True, text=True).stdout
    found = []
    for line in out.splitlines():
        first = line.strip().split(' ', 1)[0].replace('\\', '/').rsplit('/', 1)[-1]
        if first.lower() in SHELLS:
            continue
        if any(name.lower() in line.lower() for name in names):
            found.append(line)
    return found


def stop_bottle(prefix):
    subprocess.run([str(ENGINE / 'bin/wineserver'), '-k'], env=wine_env(prefix),
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def patch_reshade(dll, spec):
    data = bytearray(dll.read_bytes())
    current = hashlib.sha256(data).hexdigest()
    patch = spec['patch']
    if current == patch['patched_sha256']:
        return False
    if current != spec['dll_sha256']:
        raise VRError('ReShade64.dll is not the pinned ReShade 6.8.0 build; not patching it.')
    at = patch['file_offset']
    old, new = bytes.fromhex(patch['old']), bytes.fromhex(patch['new'])
    if bytes(data[at:at + len(old)]) != old:
        raise VRError('ReShade64.dll does not have the expected code at the patch site.')
    data[at:at + len(new)] = new
    if hashlib.sha256(data).hexdigest() != patch['patched_sha256']:
        raise VRError('Patched ReShade64.dll did not come out as expected.')
    tmp = dll.with_suffix('.dll.soju-tmp')
    tmp.write_bytes(bytes(data))
    tmp.replace(dll)
    return True


def fix_settings(saves):
    settings = saves / 'Settings.json'
    data = json.loads(settings.read_text(encoding='utf-8-sig'))
    if data.get('Screen Resolution (Windowed)') == '1920x1080' and str(data.get('Window Mode')) == '0':
        return
    data['Screen Resolution (Windowed)'] = '1920x1080'
    data['Window Mode'] = 0
    settings.write_text(json.dumps(data, indent=4), encoding='utf-8')


def write_runtime_routing(m):
    runtime = VR / 'runtime'
    openxr = BOTTLE / 'drive_c/openxr'
    openxr.mkdir(parents=True, exist_ok=True)
    shutil.copy2(runtime / 'wine/x86_64-windows/wineopxdms.dll', openxr / 'wineopxdms.dll')
    (openxr / 'wineopxdms.json').write_text(json.dumps({
        'file_format_version': '1.0.0',
        'runtime': {'name': 'Soju wineopenxr (D3DMetal)', 'library_path': 'C:\\openxr\\wineopxdms.dll'},
    }, indent=2) + '\n')
    # The bridge applies these to its own (macOS) side when it loads; the
    # Windows OpenXR loader keeps reading the registry below.
    (VR / 'logs').mkdir(parents=True, exist_ok=True)
    (VR / 'bridge.env').write_text('XR_RUNTIME_JSON=' + str(runtime / 'runtime/oxrsys-runtime.json') + '\n'
                                   'OXR_BRIDGE_LOG=' + str(VR / 'logs/bridge.log') + '\n')
    wine(BOTTLE, ['reg', 'add', 'HKLM\\Software\\Khronos\\OpenXR\\1', '/v', 'ActiveRuntime', '/t', 'REG_SZ',
                  '/d', 'C:\\openxr\\wineopxdms.json', '/f'],
         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)


def setup(refresh=False):
    m = manifest()
    if not (ENGINE / 'bin/wine').is_file():
        raise VRError('The Soju engine is not installed. Run: soju install')
    if not (SOURCE / GAME_REL / 'D2R.exe').is_file() and not (BOTTLE / GAME_REL / 'D2R.exe').is_file():
        raise VRError('Diablo II: Resurrected is not installed in the Battle.net bottle.')
    if running(['D2R.exe', 'D2RLoader.exe']):
        raise VRError('Close Diablo II: Resurrected first.')

    kept = VR / 'saves-kept'
    if refresh and BOTTLE.exists():
        if running(['Battle.net.exe']):
            raise VRError('Close Battle.net first, so the copy is consistent.')
        stop_bottle(BOTTLE)
        # Offline characters made in VR (and the VR video settings) live only
        # in the copy: keep them across the new copy.
        saves = BOTTLE / SAVES_REL
        if saves.is_dir() and not kept.exists():
            saves.rename(kept)
        say('Removing the old VR copy…')
        shutil.rmtree(BOTTLE)
        STATE.unlink(missing_ok=True)
    if not BOTTLE.exists():
        build = game_build(SOURCE)
        if build != m['game_build']:
            raise VRError('D2R VR Unleashed supports D2R ' + m['game_build'] + '; this install is '
                          + str(build) + '.')
        if running(['Battle.net.exe']):
            raise VRError('Close Battle.net first, so the copy is consistent.')
        say('Copying the Battle.net bottle (APFS copy, shares disk space until files change)…')
        VR.mkdir(parents=True, exist_ok=True)
        source = SOURCE.resolve()
        result = subprocess.run(['/bin/cp', '-c', '-R', str(source), str(BOTTLE)], capture_output=True, text=True)
        if result.returncode:
            shutil.rmtree(BOTTLE, ignore_errors=True)
            raise VRError('Could not make an APFS copy of the bottle (the bottle must be on an APFS volume): '
                          + result.stderr.strip())
        if kept.is_dir():
            saves = BOTTLE / SAVES_REL
            shutil.rmtree(saves, ignore_errors=True)
            saves.parent.mkdir(parents=True, exist_ok=True)
            kept.rename(saves)
            say('Kept the saves and settings of the previous VR copy.')
    elif game_build(BOTTLE) != m['game_build']:
        raise VRError('The VR copy has D2R ' + str(game_build(BOTTLE)) + '; D2R VR Unleashed supports '
                      + m['game_build'] + '.')

    state = {}
    try:
        state = json.loads(STATE.read_text())
    except (OSError, ValueError):
        pass
    if state.get('recipe') == recipe(m) and (BOTTLE / GAME_REL / 'D2RLoader.exe').is_file():
        say('D2R VR is already set up. Play with: soju vr play')
        return 0

    runtime_archive = fetch(m['runtime'], 'soju-vr-runtime.tar.xz', 'the VR runtime')
    python_zip = fetch(m['python'], 'python-embed-amd64.zip', 'Windows Python (python.org)')
    mod_zip = fetch(m['mod'], 'd2r-pcvr-windows.zip', m['mod']['name'])
    loader_zip = fetch(m['loader'], m['loader']['file'], 'D2RLoader (d2rloader.net)')
    reshade_exe = fetch(m['reshade'], 'ReShade_Setup_Addon.exe', 'ReShade (reshade.me)')

    for name in ('runtime', 'python', 'mod', 'deps'):
        shutil.rmtree(VR / name, ignore_errors=True)
    safe_extract_tar(runtime_archive, VR / 'runtime')
    safe_extract_zip(python_zip, VR / 'python')
    safe_extract_zip(mod_zip, VR / 'mod')
    mod = VR / 'mod' / m['mod']['folder']
    deps = VR / 'deps'
    (deps / 'reshade-6.8').mkdir(parents=True)
    shutil.copy2(loader_zip, deps / m['loader']['file'])
    with zipfile.ZipFile(reshade_exe) as z:
        dll = z.read(m['reshade']['member'])
    if hashlib.sha256(dll).hexdigest() != m['reshade']['dll_sha256']:
        raise VRError('ReShade64.dll inside the ReShade setup did not match its pin.')
    (deps / 'reshade-6.8/ReShade64.dll').write_bytes(dll)
    # The embeddable Python only imports from its ._pth paths; add the mod's.
    (VR / 'python/python312._pth').write_text('python312.zip\r\n.\r\n' + winpath(mod / 'steamvr') + '\r\n')

    game = BOTTLE / GAME_REL
    off = game / 'd3d12.dll.soju-vr-off'
    if off.exists() and not (game / 'd3d12.dll').exists():
        off.rename(game / 'd3d12.dll')   # the installer checks the stock game first
    stop_bottle(BOTTLE)
    say('Installing D2R VR Unleashed into the VR copy…')
    args = [winpath(VR / 'python/python.exe'), winpath(mod / 'steamvr/install.py'),
            '--game', 'C:\\Program Files (x86)\\Diablo II Resurrected',
            '--saves', 'C:\\users\\crossover\\Saved Games\\Diablo II Resurrected',
            '--deps', winpath(deps), '--plugin', winpath(mod / 'bin/d2rl-pcvr.dll'),
            '--openxr-loader', winpath(mod / 'bin/openxr_loader.dll'), '--casclib', winpath(mod / 'bin/CascLib.dll'),
            '--backup-dir', winpath(VR / 'backups'), '--store', 'battlenet', '--experimental-battlenet']
    if (game / 'D2RLoader.exe').is_file():
        args.append('--preserve-settings')
    result = wine(BOTTLE, args, capture_output=True, text=True)
    log = VR / 'logs/install.log'
    log.parent.mkdir(parents=True, exist_ok=True)
    log.write_text(result.stdout + result.stderr)
    if result.returncode or 'Installed offline VR mod' not in result.stdout:
        tail = [line for line in (result.stdout + result.stderr).splitlines() if line.strip()][-3:]
        raise VRError('The mod installer stopped: ' + ' | '.join(tail) + ' (log: ' + str(log) + ')')

    patch_reshade(game / 'mods' / m['mod']['mod_name'] / 'reshade/ReShade64.dll', m['reshade'])
    if (game / 'd3d12.dll').exists():
        (game / 'd3d12.dll').rename(off)
    fix_settings(BOTTLE / SAVES_REL)
    write_runtime_routing(m)
    stop_bottle(BOTTLE)
    STATE.write_text(json.dumps({'recipe': recipe(m), 'game_build': game_build(BOTTLE),
                                 'mod': m['mod']['name']}, indent=2) + '\n')
    say('D2R VR is set up. Play with: soju vr play')
    say('Use an OFFLINE character. Start the OXRSys client on your headset (or the OXRSys Simulator) to see it.')
    say('If D2R says you have not been online for 30 days: play D2R online once from Battle.net, '
        'then run: soju vr setup --refresh (VR characters are kept).')
    return 0


def status():
    m = manifest()
    state = {}
    try:
        state = json.loads(STATE.read_text())
    except (OSError, ValueError):
        pass
    return {
        'installed': BOTTLE.is_dir() and (BOTTLE / GAME_REL / 'D2RLoader.exe').is_file(),
        'current': state.get('recipe') == recipe(m),
        'game_build': game_build(BOTTLE) if BOTTLE.is_dir() else None,
        'supported_build': m['game_build'],
        'source_build': game_build(SOURCE),
        'mod': m['mod']['name'],
        'path': str(VR),
    }


def play(extra):
    info = status()
    if not info['installed'] or not info['current']:
        raise VRError('D2R VR is not set up for this Soju version. Run: soju vr setup')
    if running(['D2R.exe', 'D2RLoader.exe']):
        raise VRError('Diablo II: Resurrected is already running.')
    env = wine_env(BOTTLE)
    env['WINEDLLPATH'] = str(VR / 'runtime/wine')
    env['OXR_BRIDGE_ENV_FILE'] = str(VR / 'bridge.env')
    os.chdir(BOTTLE / GAME_REL)
    # -mod keeps D2R offline: no Battle.net game servers.
    os.execve(str(ENGINE / 'bin/wine'), [str(ENGINE / 'bin/wine'), 'D2RLoader.exe', '-mod',
                                         manifest()['mod']['mod_name']] + extra, env)


def remove(yes):
    if not VR.exists():
        say('D2R VR is not set up.')
        return 0
    if not yes:
        raise VRError('This deletes ' + str(VR) + ' (offline characters made in VR included). '
                      'Run again with --yes to confirm.')
    if running(['D2R.exe', 'D2RLoader.exe']):
        raise VRError('Close Diablo II: Resurrected first.')
    if BOTTLE.exists():
        stop_bottle(BOTTLE)
    shutil.rmtree(VR)
    say('Removed ' + str(VR) + '. Your Battle.net bottle was not changed.')
    return 0


def main(argv):
    parser = argparse.ArgumentParser(prog='soju vr')
    sub = parser.add_subparsers(dest='command', required=True)
    p = sub.add_parser('setup')
    p.add_argument('--refresh', action='store_true', help='copy the Battle.net bottle again (after a game update)')
    sub.add_parser('play')
    p = sub.add_parser('status')
    p.add_argument('--json', action='store_true')
    p = sub.add_parser('remove')
    p.add_argument('--yes', action='store_true')
    args, extra = parser.parse_known_args(argv)
    try:
        if args.command == 'setup':
            return setup(args.refresh)
        if args.command == 'play':
            return play(extra)
        if args.command == 'status':
            info = status()
            if args.json:
                print(json.dumps(info))
            else:
                say('D2R VR: ' + ('ready' if info['installed'] and info['current'] else 'not set up'))
                say('  game build in the VR copy: ' + str(info['game_build']) + ' (supported '
                    + info['supported_build'] + ')')
                say('  Battle.net bottle build: ' + str(info['source_build']))
            return 0
        if args.command == 'remove':
            return remove(args.yes)
    except VRError as error:
        print('soju vr: ' + str(error), file=sys.stderr)
        return 1
    except subprocess.CalledProcessError as error:
        print('soju vr: a step failed (' + ' '.join(map(str, error.cmd[:2])) + ')', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
