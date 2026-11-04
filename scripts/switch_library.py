"""Local user-owned Switch imports. Retail files are read with the player's own keys
(prod.keys) only to show their name and icon; Soju never downloads keys or games."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import shutil
import stat
import struct
import tempfile

TYPES = {'.nro', '.nsp', '.xci'}
RETAIL_ISSUE = "Retail games are not supported by Soju's Switch runner yet."


def game_dir(base):
    return Path(base) / 'switch/games'


def key_dir(base):
    return Path(base) / 'switch/keys'


def meta_dir(base):
    return Path(base) / 'switch/meta'


def switch_home():
    return Path(os.environ.get('SOJU_SWITCH_HOME', str(Path.home() / '.local/share/soju/switch')))


def fs_tool():
    tool = switch_home() / 'soju-switch-fs'
    return tool if os.access(tool, os.X_OK) else None


KEY_LINE = re.compile(r'^\s*([A-Za-z0-9_]{1,64})\s*=\s*([0-9A-Fa-f]{32}|[0-9A-Fa-f]{64})\s*$')


def check_key_file(path, kind):
    """Validate 'name = hex' lines without ever echoing a value."""
    names = []
    with Path(path).open('r', encoding='utf-8', errors='strict') as stream:
        for number, line in enumerate(stream, 1):
            if not line.strip() or line.lstrip().startswith((';', '#')):
                continue
            match = KEY_LINE.match(line)
            if not match:
                raise ValueError(f'{kind}: line {number} is not "name = 32 or 64 hex digits".')
            names.append(match.group(1).lower())
            if number > 10000:
                raise ValueError(f'{kind}: the file is too long to be a key file.')
    if kind == 'prod.keys' and 'header_key' not in names:
        raise ValueError('prod.keys has no header_key. Dump the keys again from your own Switch.')
    return names


def import_keys(base, prod, title_keys=None):
    names = check_key_file(prod, 'prod.keys')
    if title_keys:
        check_key_file(title_keys, 'title.keys')
    directory = key_dir(base)
    directory.mkdir(parents=True, exist_ok=True, mode=0o700)
    os.chmod(directory, 0o700)
    for source, name in [(prod, 'prod.keys'), (title_keys, 'title.keys')]:
        if not source:
            continue
        fd, temporary = tempfile.mkstemp(prefix='.keys-', dir=directory)
        try:
            with os.fdopen(fd, 'wb') as output, Path(source).open('rb') as stream:
                os.fchmod(output.fileno(), 0o600)
                shutil.copyfileobj(stream, output)
            os.replace(temporary, directory / name)
        except BaseException:
            Path(temporary).unlink(missing_ok=True)
            raise
    # Names change once keys exist: drop cached metadata so titles are read again.
    shutil.rmtree(meta_dir(base), ignore_errors=True)
    return len(names)


def retail_info(base, path, uid):
    """Name, icon, kind and version of an NSP/XCI, cached per file id. None without keys/tool."""
    cache = meta_dir(base) / (uid.split(':', 1)[1] + '.json')
    try:
        return json.loads(cache.read_text())
    except (OSError, ValueError):
        pass
    tool, prod = fs_tool(), key_dir(base) / 'prod.keys'
    if not tool or not prod.is_file():
        return None
    cache.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    icon = cache.with_suffix('.jpg')
    command = [str(tool), 'title', str(path), '--keys', str(prod), '--icon-out', str(icon)]
    title_keys = key_dir(base) / 'title.keys'
    if title_keys.is_file():
        command[4:4] = ['--title-keys', str(title_keys)]
    try:
        done = subprocess.run(command, capture_output=True, text=True, timeout=120)
    except (OSError, subprocess.TimeoutExpired):
        return None
    if done.returncode != 0:
        info = {'error': (done.stderr.strip() or 'could not read this file')[:300]}
    else:
        try:
            info = json.loads(done.stdout)
        except ValueError:
            return None
        if info.get('icon') and not icon.is_file():
            info['icon'] = None
    fd, temporary = tempfile.mkstemp(prefix='.meta-', dir=cache.parent)
    with os.fdopen(fd, 'w') as output:
        json.dump(info, output, ensure_ascii=False)
    os.replace(temporary, cache)
    return info


def digest(path):
    value = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            value.update(chunk)
    return value.hexdigest()


def title(path):
    if path.suffix.lower() == '.nro':
        with path.open('rb') as stream:
            header = stream.read(0x80)
            if len(header) == 0x80 and header[0x10:0x14] == b'NRO0':
                size = struct.unpack_from('<I', header, 0x18)[0]
                total = os.fstat(stream.fileno()).st_size
                if 0x80 <= size <= total - 0x38:
                    stream.seek(size)
                    assets = stream.read(0x38)
                    if assets[:8] == b'ASET\0\0\0\0':
                        offset, length = struct.unpack_from('<QQ', assets, 0x18)
                        if offset >= 0x38 and length >= 0x300 and offset <= total - size and length <= total - size - offset:
                            stream.seek(size + offset)
                            # NACP has 16 language entries, each name + publisher.
                            for _ in range(min(16, length // 0x300)):
                                entry = stream.read(0x300)
                                name = entry[:0x200].split(b'\0', 1)[0].decode('utf-8', errors='replace').strip()
                                if name:
                                    return ''.join(c for c in name if c.isprintable())[:300] or path.name
    return path.name


def games(base):
    result = []
    seen = set()
    for path in sorted(game_dir(base).glob('*')):
        if path.is_symlink() or not path.is_file() or path.suffix.lower() not in TYPES:
            continue
        uid = 'switch:' + digest(path)[:16]
        if uid in seen:
            continue
        seen.add(uid)
        game = dict(id=uid, platform='switch', title=title(path), install_path=str(path.absolute()),
                    artwork=None, issue=None if path.suffix.lower() == '.nro' else RETAIL_ISSUE,
                    arguments=['play', str(path.absolute())])
        if path.suffix.lower() in ('.nsp', '.xci'):
            info = retail_info(base, path, uid)
            if info is None:
                game['issue'] = RETAIL_ISSUE + ' Import your own keys (soju switch import-keys) to show its name.'
            elif info.get('error'):
                game['issue'] = RETAIL_ISSUE + ' ' + info['error']
            else:
                if info.get('name'):
                    game['title'] = ''.join(c for c in info['name'] if c.isprintable())[:300]
                if info.get('icon'):
                    game['artwork'] = info['icon']
                game['switch'] = {k: info.get(k) for k in ('title_id', 'kind', 'version', 'display_version', 'publisher')}
        result.append(game)
    return result


def import_file(base, source):
    source = Path(source)
    if source.suffix.lower() not in TYPES:
        raise ValueError('Choose an .nro, .nsp or .xci file.')
    directory = game_dir(base)
    directory.mkdir(parents=True, exist_ok=True, mode=0o700)
    target = directory / source.name
    # Publish only a complete private copy, without replacing an existing name.
    with source.open('rb') as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise ValueError('Choose a regular game file.')
        fd, temporary = tempfile.mkstemp(prefix='.import-', dir=directory)
        try:
            with os.fdopen(fd, 'wb') as output:
                os.fchmod(output.fileno(), 0o600)
                shutil.copyfileobj(stream, output, 1024 * 1024)
            os.link(temporary, target)
        finally:
            os.unlink(temporary)
    return 'switch:' + digest(target)[:16]


def remove(base, uid):
    if not re.fullmatch(r'switch:[0-9a-f]{16}', uid):
        raise ValueError('Choose a Switch game ID from the list.')
    matches = [p for p in game_dir(base).glob('*') if not p.is_symlink() and p.is_file()
               and p.suffix.lower() in TYPES and 'switch:' + digest(p)[:16] == uid]
    if not matches:
        raise ValueError('Switch game not found.')
    for path in matches:
        path.unlink()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='action', required=True)
    commands.add_parser('import').add_argument('file')
    commands.add_parser('list').add_argument('--json', action='store_true')
    commands.add_parser('remove').add_argument('id')
    keys = commands.add_parser('import-keys')
    keys.add_argument('prod_keys')
    keys.add_argument('title_keys', nargs='?')
    args = parser.parse_args()
    base = os.environ.get('SOJU_BASE', str(Path.home() / '.battlenet-macos'))
    try:
        if args.action == 'import':
            print(import_file(base, args.file))
        elif args.action == 'remove':
            remove(base, args.id)
        elif args.action == 'import-keys':
            count = import_keys(base, args.prod_keys, args.title_keys)
            print(f'Imported {count} keys (values are never shown). Stored with owner-only access.')
        else:
            rows = [{k: v for k, v in g.items() if k != 'arguments'} for g in games(base)]
            if args.json:
                print(json.dumps(rows, ensure_ascii=False))
            else:
                for row in rows:
                    print(row['id'] + '\t' + row['title'] + ('\t' + row['issue'] if row['issue'] else ''))
    except (OSError, ValueError) as error:
        parser.exit(1, 'soju switch: ' + str(error) + '\n')


if __name__ == '__main__':
    main()
