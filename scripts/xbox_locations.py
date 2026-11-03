#!/usr/bin/env python3
"""Where Xbox PC games are downloaded: the Mac (default) or a folder on an external drive.

State lives in $SOJU_BASE/xbox-locations.json (0600):
  current  the folder new downloads go to (null: the default folder in the Xbox prefix)
  roots    every folder ever chosen, each with the id of its .soju-xbox-root.json, so a
           renamed volume (or "T7 1" next to another "T7") is found again by that id
  games    product ID -> the root its download is in, so a game on an unplugged drive is
           still listed (as not connected) instead of looking "not downloaded"
APFS, Mac OS Extended (HFS+) and exFAT volumes are accepted (a game downloaded to an exFAT
SSD and started from it). FAT cannot hold the multi-GB package file, macOS mounts NTFS read-only,
and network volumes are too slow for the library scan.
Paths are only passed as argv, never through a shell.

usage: xbox_locations.py set <folder> | default | show | json
"""
import datetime
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import uuid

BASE = Path(os.environ.get('SOJU_BASE', str(Path.home() / '.battlenet-macos')))
DEFAULT = BASE / 'xbox-bottle/drive_c/XboxGames'
STATE = BASE / 'xbox-locations.json'
VOLUMES = Path('/Volumes')
SENTINEL = '.soju-xbox-root.json'
MARKER = '.soju-xbox-install.json'
CHECKPOINT = '.nexus-streaming-v1.jsonl'
FILESYSTEMS = ('apfs', 'hfs', 'exfat')
MIN_FREE = 1 << 30
PRODUCT = re.compile(r'[A-Z0-9]{12}')
FORMAT_HINT = ('Choose a folder on an APFS, Mac OS Extended or exFAT drive. To use this drive, '
               'format it as APFS or exFAT in Disk Utility (this erases it).')


class LocationError(Exception):
    pass


def tilde(path):
    """Paths in messages and logs without the user name."""
    text, home = str(path), str(Path.home())
    return '~' + text[len(home):] if text == home or text.startswith(home + '/') else text


def empty():
    return {'schema': 1, 'current': None, 'current_id': '', 'roots': [], 'games': {}}


def load():
    try:
        state = json.loads(STATE.read_text())
    except (OSError, ValueError):
        return empty()
    if not isinstance(state, dict) or state.get('schema') != 1:
        return empty()
    clean = empty()
    if isinstance(state.get('current'), str) and os.path.isabs(state['current']):
        clean['current'] = state['current']
        clean['current_id'] = str(state.get('current_id') or '')[:64]
    for entry in state.get('roots') or []:
        if isinstance(entry, dict) and isinstance(entry.get('path'), str) and os.path.isabs(entry['path']):
            clean['roots'].append({'path': entry['path'], 'id': str(entry.get('id') or '')[:64]})
    for pid, game in (state.get('games') or {}).items():
        if PRODUCT.fullmatch(str(pid)) and isinstance(game, dict) and isinstance(game.get('root'), str):
            clean['games'][pid] = {'root': game['root'], 'root_id': str(game.get('root_id') or '')[:64],
                                   'title': str(game.get('title') or pid)[:300],
                                   'seen_at': str(game.get('seen_at') or '')[:40]}
    return clean


def save(state):
    STATE.parent.mkdir(parents=True, exist_ok=True)
    temp = STATE.with_name(STATE.name + '.' + str(os.getpid()) + '.tmp')
    fd = os.open(temp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, 'w') as stream:
        json.dump(state, stream, ensure_ascii=False, indent=1)
        stream.write('\n')
    os.replace(temp, STATE)


def mounts():
    """(mount point, filesystem, options) for every mounted volume."""
    try:
        output = subprocess.run(['/sbin/mount'], capture_output=True, text=True, timeout=10).stdout
    except (OSError, subprocess.SubprocessError):
        return []
    found = []
    for line in output.splitlines():
        match = re.match(r'^.+? on (/.*) \(([^,)]+)(?:, ([^)]*))?\)$', line)
        if match:
            found.append((match.group(1), match.group(2).strip(), [o.strip() for o in (match.group(3) or '').split(',')]))
    return found


def fs_type(path):
    """Filesystem of the volume holding path (longest matching mount point), or ''."""
    path = os.path.realpath(path)
    best = ('', '')
    for point, kind, _ in mounts():
        if (path == point or path.startswith(point.rstrip('/') + '/')) and len(point) >= len(best[0]):
            best = (point, kind)
    return best[1]


def volume_name(path):
    parts = Path(path).parts
    return parts[2] if len(parts) > 2 and parts[1] == 'Volumes' else 'this Mac'


def is_default(path):
    return os.path.realpath(path) == os.path.realpath(DEFAULT)


def inside(path, root):
    path, root = os.path.realpath(path), os.path.realpath(root)
    return path == root or path.startswith(root.rstrip('/') + '/')


def allowed(path):
    """A folder on a drive in /Volumes, in the home folder, or Soju's own folder; never a system folder."""
    real = Path(os.path.realpath(path))
    home = os.path.realpath(Path.home())
    on_drive = len(real.parts) >= 4 and real.parts[1] == 'Volumes'
    return on_drive or (inside(real, home) and str(real) != home) or inside(real, BASE)


def check_root(raw):
    """The folder new downloads would use for a user's choice, or LocationError."""
    if not isinstance(raw, str) or not raw or '\x00' in raw or not os.path.isabs(raw):
        raise LocationError('Choose a folder with its full path (for example /Volumes/T7/XboxGames).')
    if '..' in Path(raw).parts:
        raise LocationError('Choose the folder itself, without "..".')
    path = Path(os.path.realpath(raw))
    if not path.is_dir():
        raise LocationError('That folder does not exist: ' + tilde(raw))
    parts = path.parts
    if len(parts) == 3 and parts[1] == 'Volumes':
        path = path / 'XboxGames'       # a drive's top level: keep games in their own folder
    if not allowed(path):
        raise LocationError('Choose a folder on an external drive (in /Volumes) or in your home folder.')
    kind = fs_type(path if path.exists() else path.parent)
    if kind not in FILESYSTEMS:
        named = {'msdos': 'FAT', 'ntfs': 'NTFS'}.get(kind, kind or 'an unknown format')
        raise LocationError(volume_name(path) + ' uses ' + named + '. ' + FORMAT_HINT)
    return path


def writable(folder):
    probe = folder / ('.soju-write-test-' + uuid.uuid4().hex)
    try:
        probe.write_bytes(b'')
        probe.unlink()
    except OSError:
        raise LocationError('Soju cannot write to ' + tilde(folder) + '. Check the drive is not read-only.') from None


def root_id(root):
    try:
        value = json.loads((Path(root) / SENTINEL).read_text())
        return str(value.get('id') or '')[:64] if isinstance(value, dict) else ''
    except (OSError, ValueError):
        return ''


def ensure_sentinel(root):
    existing = root_id(root)
    if existing:
        return existing
    ident = uuid.uuid4().hex
    (Path(root) / SENTINEL).write_text(json.dumps({'schema': 1, 'id': ident}) + '\n')
    try:
        os.chmod(Path(root) / SENTINEL, 0o600)
    except OSError:
        pass   # drives with "ignore ownership"
    return ident


def set_current(raw):
    path = check_root(raw)
    try:
        path.mkdir(exist_ok=True)
    except OSError:
        raise LocationError('Soju cannot create ' + tilde(path) + '. Check the drive is not read-only.') from None
    if not inside(path, path.parent):   # created as a link by someone else meanwhile
        raise LocationError('That folder is a link. Choose a real folder.')
    writable(path)
    state = load()
    if is_default(path):
        state['current'], state['current_id'] = None, ''
    else:
        ident = ensure_sentinel(path)
        state['current'], state['current_id'] = str(path), ident
        # Roots are told apart by their id, never by path: another drive with the same
        # name keeps its own entry (and its games stay listed as not connected).
        state['roots'] = [r for r in state['roots'] if r['id'] != ident and (r['id'] or r['path'] != str(path))]
        state['roots'].append({'path': str(path), 'id': ident})
    save(state)
    return path


def use_default():
    state = load()
    state['current'], state['current_id'] = None, ''
    save(state)


def relocate(state, entry, warnings):
    """A root's current path: as saved, or found again by its id on a renamed volume."""
    path = entry['path']
    if Path(path).is_dir() and (not entry['id'] or root_id(path) == entry['id']):
        return path
    try:
        relative = Path(path).relative_to(VOLUMES)
    except ValueError:
        return None
    if not entry['id'] or not relative.parts:
        return None
    rest = Path(*relative.parts[1:])
    # Only mounted local volumes Soju can use: no network shares (a stale one can hang an open),
    # no NTFS, and not the "Macintosh HD" link to the startup disk.
    volumes = sorted(Path(point) for point, kind, _ in mounts()
                     if kind in FILESYSTEMS and Path(point).parent == VOLUMES)
    for volume in volumes:
        candidate = volume / rest
        if str(candidate) != path and root_id(candidate) == entry['id'] and inside(candidate, volume):
            old, new = path, str(candidate)
            entry['path'] = new
            if state['current_id'] == entry['id'] or (not state['current_id'] and state['current'] == old):
                state['current'] = new
            for game in state['games'].values():
                if game.get('root_id') == entry['id'] or (not game.get('root_id') and game['root'] == old):
                    game['root'] = new
            warnings.append('Xbox games on ' + volume_name(old) + ' are now found on ' + volume_name(new) + '.')
            return new
    return None


def roots(state=None, warnings=None):
    """[(path or None when not connected, saved path, root id)], the current location first, the Mac last."""
    state = load() if state is None else state
    warnings = [] if warnings is None else warnings
    found, seen = [], set()
    entries = sorted(state['roots'], key=lambda r: not (r['id'] == state['current_id'] or r['path'] == state['current']))
    for entry in entries:
        saved = entry['path']
        path = relocate(state, entry, warnings)
        if path and path not in seen:
            seen.add(path)
            found.append((path, path, entry['id']))
        elif not path:
            found.append((None, saved, entry['id']))
    if str(DEFAULT) not in seen:
        found.append((str(DEFAULT), str(DEFAULT), ''))
    return found


def folder_state(folder):
    if (folder / MARKER).is_file():
        return 'complete'
    if (folder / CHECKPOINT).is_file():
        return 'partial'
    return None


def scan(state=None, warnings=None):
    """{pid: (folder, 'complete'|'partial')} on connected roots, plus {pid: (saved root, 'missing')}
    for indexed games whose drive is not connected. Updates the index (written only if it changed)."""
    state = load() if state is None else state
    warnings = [] if warnings is None else warnings
    before = json.dumps(state, sort_keys=True)
    games = {}
    listed_paths, listed_ids, ids = set(), set(), {}
    for path, saved, ident in roots(state, warnings):
        if path is None:
            continue
        root = Path(path)
        try:
            folders = sorted(p for p in root.iterdir() if PRODUCT.fullmatch(p.name))
        except OSError:
            if not is_default(root):
                warnings.append('Xbox games on ' + volume_name(path) + ' could not be read. Reconnect the drive and refresh.')
            continue
        # Only a root that was actually listed may drop games that are no longer on it.
        listed_paths.add(path)
        ids[path] = ident
        if ident:
            listed_ids.add(ident)
        for folder in folders:
            # A game folder must be a real folder inside its root (no links out of it).
            if folder.is_symlink() or not folder.is_dir() or not inside(folder, root):
                continue
            kind = folder_state(folder)
            if not kind:
                continue
            pid = folder.name
            if pid in games:
                if games[pid][1] == 'complete' or kind != 'complete':
                    warnings.append('Xbox game ' + pid + ' is in two download folders; using ' +
                                    tilde(games[pid][0]) + '.')
                    continue
            games[pid] = (folder, kind)
    now = datetime.datetime.now(datetime.timezone.utc).isoformat(timespec='seconds')
    for pid, (folder, kind) in games.items():
        root = str(folder.parent)
        if is_default(root):
            state['games'].pop(pid, None)
            continue
        game = state['games'].get(pid) or {'title': pid}
        if kind == 'complete' and game.get('title', pid) == pid:
            try:
                game['title'] = str(json.loads((folder / MARKER).read_text()).get('title') or pid)[:300]
            except (OSError, ValueError, AttributeError):
                pass
        if game.get('root') != root or game.get('root_id') != ids.get(root, ''):
            game.update(root=root, root_id=ids.get(root, ''), seen_at=now)
        state['games'][pid] = game
    for pid, game in list(state['games'].items()):
        if pid in games:
            continue
        gone = game['root_id'] in listed_ids if game.get('root_id') else game['root'] in listed_paths
        if gone:
            del state['games'][pid]            # its drive is here but the folder is gone: deleted
        else:
            games[pid] = (Path(game['root']), 'missing')
    if json.dumps(state, sort_keys=True) != before:
        try:
            save(state)
        except OSError:
            pass
    return games


def remember(pid, folder, title):
    """Record a finished download so it stays listed while its drive is unplugged."""
    state = load()
    if is_default(folder.parent):
        state['games'].pop(pid, None)
    else:
        state['games'][pid] = {'root': str(folder.parent), 'root_id': root_id(folder.parent), 'title': str(title)[:300],
                               'seen_at': datetime.datetime.now(datetime.timezone.utc).isoformat(timespec='seconds')}
    save(state)


def not_connected(root):
    return ('The drive with this game (' + volume_name(root) + ') is not connected. '
            'Connect it, then refresh the library.')


def locate(pid):
    """(folder, 'complete'|'partial'|'missing'|None) for a product ID."""
    found = scan().get(pid)
    return found if found else (None, None)


def current_root(state=None):
    state = load() if state is None else state
    return state['current'] or str(DEFAULT)


def destination(pid):
    """The folder a download of pid goes to: an existing (partial) one, else the current location."""
    folder, kind = locate(pid)
    if kind == 'missing':
        raise LocationError(not_connected(folder))
    if folder is not None:
        return folder
    state = load()
    root = Path(current_root(state))
    if not is_default(root):
        if not allowed(root):
            raise LocationError('The saved download location is not allowed. Choose it again in Accounts.')
        roots(state, [])   # finds a renamed drive by its id
        root = Path(current_root(state))
        if not root.is_dir() or (state['current_id'] and root_id(root) != state['current_id']):
            raise LocationError('The download location on ' + volume_name(root) + ' is not connected. '
                                'Connect the drive, or choose another location in Accounts.')
        kind = fs_type(root)
        if kind not in FILESYSTEMS:
            raise LocationError(volume_name(root) + ' cannot hold Xbox games. ' + FORMAT_HINT)
    else:
        root.mkdir(parents=True, exist_ok=True)
    try:
        if shutil.disk_usage(root).free < MIN_FREE:
            raise LocationError('There is less than 1 GB free on ' + volume_name(root) +
                                '. Free up space or choose another download location in Accounts.')
    except OSError:
        pass
    folder = root / pid
    if folder.is_symlink() or (folder.exists() and not inside(folder, root)):
        raise LocationError('The game folder in ' + tilde(root) + ' is a link. Remove it and download again.')
    return folder


def describe(state=None):
    state = load() if state is None else state
    root = current_root(state)
    info = {'path': tilde(root), 'volume': volume_name(root), 'default': is_default(root),
            'available': Path(root).is_dir() or is_default(root), 'free_bytes': None}
    try:
        info['free_bytes'] = shutil.disk_usage(root if Path(root).is_dir() else BASE).free
    except OSError:
        pass
    return info


def main(argv):
    action = argv[1] if len(argv) > 1 else 'show'
    try:
        if action == 'set' and len(argv) == 3:
            path = set_current(argv[2])
            print('New Xbox downloads go to ' + tilde(path) + ' (' + volume_name(path) + ').')
            print('Games already downloaded stay where they are.')
        elif action == 'default' and len(argv) == 2:
            use_default()
            print('New Xbox downloads go to this Mac (' + tilde(DEFAULT) + ').')
        elif action == 'show' and len(argv) == 2:
            warnings = []
            info = describe()
            print('Download location: ' + info['path'] + ('' if info['available'] else ' (not connected)'))
            for path, saved, _ in roots(load(), warnings):
                if path is None:
                    print('  not connected: ' + volume_name(saved) + ' (' + tilde(saved) + ')')
            for line in warnings:
                print(line)
        elif action == 'json' and len(argv) == 2:
            warnings = []
            state = load()
            games = scan(state, warnings)
            listed = [{'path': tilde(saved), 'volume': volume_name(saved), 'connected': path is not None,
                       'partial': sum(1 for f, k in games.values() if k == 'partial' and str(f.parent) == path)}
                      for path, saved, _ in roots(state, [])]
            print(json.dumps({'current': describe(state), 'roots': listed, 'warnings': warnings}, ensure_ascii=False))
        else:
            print(__doc__.strip().splitlines()[-1], file=sys.stderr)
            return 64
        return 0
    except (LocationError, OSError) as error:
        print('soju: ' + tilde(str(error)) if isinstance(error, LocationError) else
              'soju: ' + str(error).replace(str(Path.home()), '~'), file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main(sys.argv))
