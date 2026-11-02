#!/usr/bin/env python3
"""Prepare the Xbox prefix (xbox-bottle) so GDK games can start.

- boots the prefix with Soju's CX engine (D3DMetal from the user's GPTK);
- copies the Xbox runtime DLLs (fetch-xbox-runtime.py) into system32 as native
  DLLs (Wine treats a file still carrying its "Wine builtin DLL" marker as a
  placeholder, so the marker is cleared);
- sets the DLL overrides and the Gaming Services version games check;
- installs Microsoft's VC++ 2015-2022 runtime (downloaded from Microsoft and
  checked by SHA-256; never redistributed), whose concrt140 games need.
A marker records what was applied, so later launches skip all of this.
"""
import hashlib
import json
import os
import signal
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
BASE = Path(os.environ.get('SOJU_BASE', str(Path.home() / '.battlenet-macos')))
ENGINE = Path(os.environ.get('ENGINE', str(BASE / 'cx26-engine')))
PREFIX = BASE / 'xbox-bottle'
RUNTIME = BASE / 'xbox-runtime'
RUNNER = ROOT / 'scripts/xbox-wine.sh'
MARKER = PREFIX / '.soju-xbox-prefix'
BUILTIN = b'Wine builtin DLL'
VC = dict(version='14.44.35211.0', sha256='cc0ff0eb1dc3f5188ae6300faef32bf5beeba4bdd6e8e445a9184072096b713b',
          url='https://download.visualstudio.microsoft.com/download/pr/bd1c8d9d-ba95-4eee-bc6e-df1fcc876373/'
              'CC0FF0EB1DC3F5188AE6300FAEF32BF5BEEBA4BDD6E8E445A9184072096B713B/VC_redist.x64.exe')


class PrefixError(Exception):
    pass


def manifest():
    return json.loads((ROOT / 'resources/xbox-runtime.json').read_text())


def overrides(names):
    keys = [n[:-4] for n in names] + ['concrt140']
    settings = [['reg', 'add', 'HKCU\\Software\\Wine\\DllOverrides', '/v', k, '/t', 'REG_SZ', '/d', 'native,builtin', '/f']
                for k in keys]
    # Gaming Services 38.117.27001.0, as the Nexus runtime writes it; some games check it.
    settings += [['reg', 'add', 'HKLM\\SOFTWARE\\Microsoft\\GamingServices', '/v', v, '/t', 'REG_QWORD',
                  '/d', '0x0026007569790000', '/f', '/reg:64'] for v in ('CurrentVersion', 'AvailableVersion')]
    settings += [['reg', 'add', 'HKCU\\Software\\Wine\\WineDbg', '/v', 'ShowCrashDialog', '/t', 'REG_DWORD', '/d', '0', '/f']]
    return settings


def state(m):
    return hashlib.sha256(json.dumps([m['dlls'], overrides(m['dlls']), VC['sha256']], sort_keys=True).encode()).hexdigest()


def engine_problem():
    if not os.access(ENGINE / 'bin/wine', os.X_OK):
        return 'Set up Xbox PC again in Platforms: it installs the engine Xbox PC games run on.'
    if not (ENGINE / 'lib/external/libd3dshared.dylib').is_file():
        return "Add Apple's Game Porting Toolkit in Platforms before playing."
    return None


def ready():
    try:
        return engine_problem() is None and MARKER.read_text().strip() == state(manifest()) and \
            subprocess.run([sys.executable, str(ROOT / 'scripts/fetch-xbox-runtime.py'), '--check'],
                           capture_output=True).returncode == 0
    except OSError:
        return False


def wine(args, timeout=600):
    env = {k: v for k, v in os.environ.items() if not k.startswith(('WINE', 'DYLD_'))}
    env.update(SOJU_BASE=str(BASE), ENGINE=str(ENGINE))
    result = subprocess.run(['/bin/sh', str(RUNNER)] + args, env=env, capture_output=True, timeout=timeout)
    return result.returncode


def install_dlls(names):
    system32 = PREFIX / 'drive_c/windows/system32'
    system32.mkdir(parents=True, exist_ok=True)
    for name in names:
        data = bytearray((RUNTIME / name).read_bytes())
        if len(data) >= 80 and data[64:80] == BUILTIN:
            data[64:80] = bytes(16)
        target = system32 / name
        if target.is_file() and target.read_bytes() == bytes(data):
            continue
        temporary = target.with_name(target.name + '.soju')
        temporary.write_bytes(bytes(data))
        temporary.chmod(0o644)
        os.replace(temporary, target)


def vc_runtime():
    cache = BASE / 'build'
    cache.mkdir(parents=True, exist_ok=True)
    exe = cache / ('vc_redist.x64-' + VC['version'] + '.exe')
    def good():
        try:
            return hashlib.sha256(exe.read_bytes()).hexdigest() == VC['sha256']
        except OSError:
            return False
    if not good():
        print('Downloading the Microsoft VC++ runtime…', flush=True)
        subprocess.run(['/usr/bin/curl', '--fail', '--silent', '--show-error', '--location', '--proto', '=https',
                        '--proto-redir', '=https', '--max-time', '600', VC['url'], '--output', str(exe)], check=True)
        if not good():
            exe.unlink(missing_ok=True)
            raise PrefixError('The VC++ runtime download did not match its checksum.')
    print('Installing the VC++ runtime into the Xbox prefix…', flush=True)
    code = wine(['Z:' + str(exe).replace('/', '\\'), '/install', '/quiet', '/norestart'], timeout=900)
    # 1638: a same or newer version is installed; 3010: installed, restart pending.
    if code not in (0, 1638, 3010):
        raise PrefixError('The VC++ runtime did not install (code ' + str(code) + '). Run Diagnose and retry.')


def prepare():
    problem = engine_problem()
    if problem:
        raise PrefixError(problem)
    m = manifest()
    if subprocess.run([sys.executable, str(ROOT / 'scripts/fetch-xbox-runtime.py')]).returncode != 0:
        raise PrefixError('The Xbox game runtime could not be installed.')
    if ready():
        print('Xbox prefix ready.')
        return
    PREFIX.mkdir(parents=True, exist_ok=True)
    if not (PREFIX / 'system.reg').is_file():
        print('Creating the Xbox prefix…', flush=True)
        wine(['wineboot', '-u'])
        if not (PREFIX / 'system.reg').is_file():
            raise PrefixError('The Xbox prefix could not be created.')
    install_dlls(m['dlls'])
    for args in overrides(m['dlls']):
        if wine(args) != 0:
            raise PrefixError('Could not apply ' + ' '.join(args[2:5]) + ' in the Xbox prefix.')
    vc_runtime()
    MARKER.write_text(state(m) + '\n')
    print('Xbox prefix ready.')


if __name__ == '__main__':
    # Stop from Soju (SIGTERM to the process group): exit normally so temporary
    # folders and partial downloads are cleaned up.
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(143))
    try:
        if sys.argv[1:] == ['--check']:
            sys.exit(0 if ready() else 1)
        prepare()
    except (PrefixError, OSError, ValueError, subprocess.SubprocessError) as error:
        print('soju: ' + str(error), file=sys.stderr)
        sys.exit(1)
