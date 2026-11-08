#!/usr/bin/env python3
"""Native Mac games from Didim Labs: install from a downloaded zip, list, remove.

A game zip holds one .app whose Contents/Resources/soju-game.json names it:
  {"schema": 1, "id": "dont-sell-the-cat", "title": "Don't Sell the Cat",
   "publisher": "Didim Labs", "version": "1.0", "cover": "soju-cover.png"}
Games install to $SOJU_BASE/didim-games/<id>/. No network, no accounts.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ID = re.compile(r'[a-z0-9][a-z0-9-]{0,63}')
STORE = os.environ.get('SOJU_DIDIM_STORE', 'https://didimlabs.com')
KEYCHAIN_SERVICE = 'Soju Didim Labs license'
KEY = re.compile(r'[A-Za-z0-9-]{8,100}')
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


def install(archive, expect=None):
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
        if expect and item['id'] != expect:
            raise ValueError('The download is a different game than the one bought.')
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


def saved_key(uid):
    found = subprocess.run(['/usr/bin/security', 'find-generic-password', '-s', KEYCHAIN_SERVICE, '-a', uid, '-w'],
                           capture_output=True, text=True)
    return found.stdout.strip() if found.returncode == 0 else ''


def save_key(uid, key):
    # Stored in the login keychain so a reinstall or update needs no key; never written to logs or files.
    subprocess.run(['/usr/bin/security', 'add-generic-password', '-U', '-s', KEYCHAIN_SERVICE, '-a', uid,
                    '-l', 'Soju: ' + uid, '-w', key], check=True, capture_output=True)


def fetch_json(url, body):
    import urllib.request, urllib.error
    request = urllib.request.Request(url, data=json.dumps(body).encode(), method='POST',
                                     headers={'Content-Type': 'application/json', 'User-Agent': 'Soju'})
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.loads(response.read(1024 * 1024))
    except urllib.error.HTTPError as error:
        try: message = json.loads(error.read(65536)).get('error')
        except (ValueError, AttributeError): message = None
        raise ValueError(message or 'The store answered with an error (' + str(error.code) + ').')
    except urllib.error.URLError:
        raise ValueError('Could not reach the store. Check the internet connection.')


def activate(uid):
    """Download and install a bought game. The key comes from SOJU_DIDIM_KEY or the keychain."""
    if not ID.fullmatch(uid or ''): raise ValueError('Choose a game.')
    key = os.environ.pop('SOJU_DIDIM_KEY', '').strip() or saved_key(uid)
    if not key: raise ValueError('Enter the license key from your purchase email.')
    if not KEY.fullmatch(key): raise ValueError('That license key does not look right.')
    print('Checking the license key…', flush=True)
    info = fetch_json(STORE + '/api/didim/download', {'game': uid, 'key': key})
    name, size, digest, url = info.get('name'), info.get('size'), info.get('sha256'), info.get('url')
    if not (isinstance(name, str) and name.endswith('.zip') and '/' not in name and isinstance(size, int)
            and isinstance(digest, str) and re.fullmatch(r'[0-9a-f]{64}', digest)
            and isinstance(url, str) and url.startswith('https://')):
        raise ValueError('The store sent an unexpected answer.')
    save_key(uid, key)
    download_install(name, size, digest, url, expect=uid)


def download_install(name, size, digest, url, expect):
    """Stream a zip into didim-games/.downloads, check its size and SHA-256, then install it."""
    import urllib.request
    folder = games_dir() / '.downloads'
    folder.mkdir(parents=True, exist_ok=True, mode=0o700)
    target = folder / name
    print('Downloading ' + name + ' (' + str(size // (1024 * 1024)) + ' MB)…', flush=True)
    sha = hashlib.sha256(); done = 0; shown = -1
    request = urllib.request.Request(url, headers={'User-Agent': 'Soju'})
    with urllib.request.urlopen(request, timeout=60) as response, target.open('wb') as out:
        while True:
            chunk = response.read(1024 * 1024)
            if not chunk: break
            out.write(chunk); sha.update(chunk); done += len(chunk)
            if done > size: raise ValueError('The download is larger than expected.')
            percent = done * 100 // size
            if percent // 5 != shown:
                shown = percent // 5
                print('Downloading… ' + str(percent) + '%', flush=True)
    try:
        if done != size or sha.hexdigest() != digest:
            raise ValueError('The download was damaged. Try again.')
        install(target, expect=expect)
    finally:
        target.unlink(missing_ok=True)


DEMO_PREFIX = 'https://github.com/BCD1210/soju/releases/download/'


def demo(uid):
    """Download and install the free demo listed for a game in the store catalog."""
    import urllib.request, urllib.error
    if not ID.fullmatch(uid or ''): raise ValueError('Choose a game.')
    print('Looking up the demo…', flush=True)
    try:
        request = urllib.request.Request(STORE + '/store/catalog.json', headers={'User-Agent': 'Soju'})
        with urllib.request.urlopen(request, timeout=30) as response:
            catalog = json.loads(response.read(512 * 1024))
    except (urllib.error.URLError, ValueError):
        raise ValueError('Could not reach the store. Check the internet connection.')
    game = next((g for g in catalog.get('games', []) if isinstance(g, dict) and g.get('id') == uid), None)
    info = game.get('demo') if game else None
    if not isinstance(info, dict): raise ValueError('This game has no demo yet.')
    demo_id, url, size, digest = info.get('id'), info.get('url'), info.get('size'), info.get('sha256')
    if not (isinstance(demo_id, str) and ID.fullmatch(demo_id) and demo_id != uid
            and isinstance(url, str) and url.startswith(DEMO_PREFIX) and url.endswith('.zip')
            and isinstance(size, int) and 0 < size < 4 * 1024 ** 3
            and isinstance(digest, str) and re.fullmatch(r'[0-9a-f]{64}', digest)):
        raise ValueError('The store listed a demo Soju cannot use.')
    download_install(url.rsplit('/', 1)[1], size, digest, url, expect=demo_id)


def remove(uid):
    if not ID.fullmatch(uid or ''): raise ValueError('Choose a game ID.')
    dest = games_dir() / uid
    if not (dest / '.soju-didim.json').is_file(): raise ValueError('This game is not installed.')
    shutil.rmtree(dest)
    print('Removed ' + uid + '. Saves stay in ~/Library/Application Support.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['install', 'activate', 'demo', 'list', 'remove'])
    parser.add_argument('target', nargs='?')
    args = parser.parse_args()
    try:
        if args.action == 'install': install(args.target or '')
        elif args.action == 'remove': remove(args.target)
        elif args.action == 'activate': activate(args.target)
        elif args.action == 'demo': demo(args.target)
        else: print(json.dumps(installed(), ensure_ascii=False))
    except (OSError, ValueError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        print('soju: ' + str(error), file=sys.stderr); sys.exit(1)


if __name__ == '__main__':
    main()
