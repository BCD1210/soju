"""Local user-owned Switch imports; no decryption or retail execution."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import struct
import tempfile

TYPES = {'.nro', '.nsp', '.xci'}
RETAIL_ISSUE = "Retail games are not supported by Soju's Switch runner yet."


def game_dir(base):
    return Path(base) / 'switch/games'


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
        result.append(dict(id=uid, platform='switch', title=title(path), install_path=str(path.absolute()),
                           artwork=None, issue=None if path.suffix.lower() == '.nro' else RETAIL_ISSUE,
                           arguments=['play', str(path.absolute())]))
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
    args = parser.parse_args()
    base = os.environ.get('SOJU_BASE', str(Path.home() / '.battlenet-macos'))
    try:
        if args.action == 'import':
            print(import_file(base, args.file))
        elif args.action == 'remove':
            remove(base, args.id)
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
