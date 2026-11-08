#!/usr/bin/env python3
"""Native Mac games from Didim Labs: install from a downloaded zip, list, remove.

A game zip holds one .app whose Contents/Resources/soju-game.json names it:
  {"schema": 1, "id": "dont-sell-the-cat", "title": "Don't Sell the Cat",
   "publisher": "Didim Labs", "version": "1.0", "cover": "soju-cover.png"}
Games install to $SOJU_BASE/didim-games/<id>/. No network, no accounts.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ID = re.compile(r'[a-z0-9][a-z0-9-]{0,63}')
MAX_MANIFEST = 64 * 1024


def base():
    return Path(os.environ.get('SOJU_BASE', str(Path.home() / '.battlenet-macos'))).expanduser().absolute()


def games_dir():
    return base() / 'didim-games'


def read_manifest(app):
    path = app / 'Contents/Resources/soju-game.json'
    if not path.is_file() or path.is_symlink():
        raise ValueError('This is not a Soju game: soju-game.json is missing.')
    data = path.read_bytes()
    if len(data) > MAX_MANIFEST:
        raise ValueError('The game manifest is too large.')
    item = json.loads(data.decode('utf-8'))
    if not isinstance(item, dict) or item.get('schema') != 1:
        raise ValueError('Unsupported game manifest.')
    if not isinstance(item.get('id'), str) or not ID.fullmatch(item['id']):
        raise ValueError('Invalid game ID in the manifest.')
    for key in ('title', 'publisher', 'version'):
        if not isinstance(item.get(key), str) or not item[key].strip() or len(item[key]) > 200:
            raise ValueError('The manifest is missing ' + key + '.')
    cover = item.get('cover')
    if cover is not None and (not isinstance(cover, str) or '/' in cover or cover.startswith('.')):
        raise ValueError('Invalid cover name in the manifest.')
    return item


def inside(root, path):
    try:
        path.resolve().relative_to(root.resolve())
        return True
    except ValueError:
        return False


def install(archive):
    archive = Path(archive).expanduser().absolute()
    if not archive.is_file() or archive.suffix.lower() != '.zip':
        raise ValueError('Choose the game .zip you downloaded.')
    target_root = games_dir()
    target_root.mkdir(parents=True, exist_ok=True, mode=0o700)
    with tempfile.TemporaryDirectory(dir=target_root, prefix='.unpack-') as tmp:
        tmp = Path(tmp)
        # ditto keeps bundle symlinks, permissions and code signatures intact.
        subprocess.run(['/usr/bin/ditto', '-x', '-k', str(archive), str(tmp)], check=True)
        for p in tmp.rglob('*'):
            if p.is_symlink() and not inside(tmp, p):
                raise ValueError('The archive contains a link outside the game.')
        apps = [p for p in tmp.iterdir() if p.suffix == '.app' and p.is_dir()]
        if len(apps) != 1:
            raise ValueError('The archive must contain exactly one .app.')
        app = apps[0]
        item = read_manifest(app)
        # Downloads carry quarantine; the bundle is checked before it is trusted.
        subprocess.run(['/usr/bin/xattr', '-dr', 'com.apple.quarantine', str(app)], check=False,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        check = subprocess.run(['/usr/bin/codesign', '--verify', '--deep', '--strict', str(app)],
                               capture_output=True, text=True)
        if check.returncode != 0:
            raise ValueError('The game signature is invalid or damaged: ' + check.stderr.strip()[:300])
        dest = target_root / item['id']
        old = target_root / ('.old-' + item['id'])
        if old.exists(): shutil.rmtree(old)
        if dest.exists(): dest.rename(old)
        dest.mkdir(mode=0o700)
        app.rename(dest / app.name)
        record = dict(item, schema=1, app=app.name)
        (dest / '.soju-didim.json').write_text(json.dumps(record, ensure_ascii=False), encoding='utf-8')
        if old.exists(): shutil.rmtree(old)
    print('Installed ' + item['title'] + ' ' + item['version'] + ' (' + item['publisher'] + ').')


def installed(soju_base=None):
    """Installed games as library entries (used by game-library.py with its own base)."""
    games = []
    root = Path(soju_base) / 'didim-games' if soju_base else games_dir()
    if not root.is_dir(): return games
    for record_path in sorted(root.glob('*/.soju-didim.json')):
        folder = record_path.parent
        if folder.name.startswith('.'): continue
        item = json.loads(record_path.read_text(encoding='utf-8'))
        if item.get('schema') != 1 or item.get('id') != folder.name or not ID.fullmatch(folder.name):
            raise ValueError('mismatched game record')
        app = folder / item['app']
        if app.suffix != '.app' or '/' in item['app'] or not app.is_dir():
            raise ValueError('missing game bundle')
        cover = app / 'Contents/Resources' / item['cover'] if item.get('cover') else None
        games.append(dict(uid=item['id'], title=item['title'], app=str(app), folder=str(folder),
                          artwork=str(cover) if cover and cover.is_file() else None,
                          version=item['version'], publisher=item['publisher']))
    return games


def remove(uid):
    if not ID.fullmatch(uid or ''): raise ValueError('Choose a game ID.')
    dest = games_dir() / uid
    if not (dest / '.soju-didim.json').is_file(): raise ValueError('This game is not installed.')
    shutil.rmtree(dest)
    print('Removed ' + uid + '. Saves stay in ~/Library/Application Support.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['install', 'list', 'remove'])
    parser.add_argument('target', nargs='?')
    args = parser.parse_args()
    try:
        if args.action == 'install': install(args.target or '')
        elif args.action == 'remove': remove(args.target)
        else: print(json.dumps(installed(), ensure_ascii=False))
    except (OSError, ValueError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        print('soju: ' + str(error), file=sys.stderr); sys.exit(1)


if __name__ == '__main__':
    main()
