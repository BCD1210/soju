#!/usr/bin/env python3
"""Read installed games locally. No account APIs, tokens, telemetry or network.

Launches are reconstructed from current manifests, never from a cached command.
All launcher arguments are passed as an argv array without a shell.
"""
import argparse
import json
import os
from pathlib import Path, PureWindowsPath
import re
import sqlite3
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent))
from game_artwork import executable_icon
from library_services import saved_accounts
import xbox_locations
import didim_games
from urllib.parse import quote

FOLDERS = {'steam': 'steam-bottle', 'epic': 'epic-bottle',
           'battlenet': 'bottle', 'gog': 'gog-bottle', 'xbox': 'xbox-bottle'}
# Steam games that stop without Apple GPTK (soju steam-gptk on): Overwatch 2, and
# Diablo II: Resurrected, whose loader needs GPTK's libd3dshared.
STEAM_GPTK_GAMES = {'2357570', '2536520'}
CLIENTS = {
    'steam': 'Program Files (x86)/Steam/steam.exe',
    'epic': 'Program Files/Epic Games/Launcher/Portal/Binaries/Win64/EpicGamesLauncher.exe',
    'battlenet': 'Program Files (x86)/Battle.net/Battle.net.exe',
    'gog': 'Program Files/GOG Galaxy/GalaxyClient.exe',
    # Xbox PC has no Windows client: `xbox.sh install` writes this marker.
    'xbox': 'ProgramData/Soju/xbox.json',
}
XBOX_ENGINE = 'Set up Xbox PC again in Platforms: it installs the engine Xbox PC games run on.'
XBOX_GPTK = "Add Apple's Game Porting Toolkit in Platforms before playing."
# Product UIDs differ from the Battle.net launch command's product codes.
BNET = {
    'osi': ('Diablo II: Resurrected', 'OSI'),
    'diablo3': ('Diablo III', 'D3'), 'fen': ('Diablo IV', 'Fen'),
    'wow': ('World of Warcraft', 'WoW'),
    's2': ('StarCraft II', 'S2'), 's1': ('StarCraft: Remastered', 'S1'),
    'w3': ('Warcraft III', 'W3'), 'hs_beta': ('Hearthstone', 'WTCG'),
    'heroes': ('Heroes of the Storm', 'Hero'), 'prometheus': ('Overwatch 2', 'Pro'),
}
MAX_METADATA = 16 * 1024 * 1024


def read_bytes(path):
    with path.open('rb') as stream:
        data = stream.read(MAX_METADATA + 1)
    if len(data) > MAX_METADATA:
        raise ValueError('metadata is too large')
    return data


def read_json(path):
    value = json.loads(read_bytes(path).decode('utf-8-sig'))
    if not isinstance(value, dict):
        raise ValueError('expected a metadata object')
    return value


def case_path(root, parts):
    """Resolve Windows' case-insensitive filenames on a case-sensitive volume."""
    for part in parts:
        if part in ('', '.'):
            continue
        if part == '..' or '\x00' in part:
            raise ValueError('invalid path component')
        target = root / part
        if not target.exists() and root.is_dir():
            target = next((p for p in root.iterdir() if p.name.casefold() == part.casefold()), target)
        root = target
    return root


def windows_path(prefix, text):
    if not isinstance(text, str) or not text or '\x00' in text:
        raise ValueError('missing Windows path')
    p = PureWindowsPath(text)
    if not re.fullmatch(r'[A-Za-z]:', p.drive) or not p.root:
        raise ValueError('expected an absolute mapped Windows drive')
    drive = p.drive[0].lower()
    root = prefix / 'drive_c' if drive == 'c' else prefix / 'dosdevices' / (drive + ':')
    return case_path(root, p.parts[1:])


def vdf(data):
    # Steam KeyValues: quoted strings, escaped quotes/backslashes, comments, objects.
    tokens = re.findall(r'"(?:\\.|[^"\\])*"|[{}]|//[^\n]*|[^\s{}"]+', data)
    tokens = [t for t in tokens if not t.startswith('//')]
    at = 0
    def value(t):
        if t.startswith('"'):
            return re.sub(r'\\(["\\])', r'\1', t[1:-1])
        return t
    def obj(depth=0, nested=False):
        nonlocal at
        if depth > 32:
            raise ValueError('KeyValues nesting too deep')
        result = {}
        while at < len(tokens):
            key = tokens[at]; at += 1
            if key == '}':
                if not nested: raise ValueError('unexpected closing brace')
                return result
            if key == '{' or at == len(tokens): raise ValueError('invalid KeyValues')
            item = tokens[at]; at += 1
            if item == '}': raise ValueError('missing value')
            result[value(key)] = obj(depth + 1, True) if item == '{' else value(item)
        if nested: raise ValueError('unclosed KeyValues object')
        return result
    return obj()


def protobuf(data):
    """Bounded wire reader for Agent's local ProductDb (no generated dependency)."""
    at = 0
    def integer():
        nonlocal at
        n = 0
        for shift in range(0, 70, 7):
            if at >= len(data): raise ValueError('truncated protobuf')
            c = data[at]; at += 1; n |= (c & 127) << shift
            if c < 128: return n
        raise ValueError('invalid varint')
    result = {}
    while at < len(data):
        tag = integer(); field, wire = tag >> 3, tag & 7
        if not field: raise ValueError('invalid protobuf field')
        if wire == 0: item = integer()
        elif wire in (1, 2, 5):
            size = integer() if wire == 2 else (8 if wire == 1 else 4)
            if size > len(data) - at: raise ValueError('truncated protobuf value')
            item = data[at:at + size]; at += size
        else: raise ValueError('unsupported protobuf wire type')
        result.setdefault(field, []).append(item)
    return result


def first_file(paths):
    return next((str(p) for p in paths if p.is_file()), None)


class Library:
    def __init__(self, base):
        self.base = Path(base).expanduser().absolute()
        self.folders = dict(FOLDERS)
        # soju steam-gptk on: Steam lives in its own bottle (scripts/steam-gptk.sh).
        if (self.base / 'steam-gptk').is_file() and \
                (self.base / 'steam-gptk-bottle/drive_c' / CLIENTS['steam']).is_file():
            self.folders['steam'] = 'steam-gptk-bottle'
        self.games = {}
        self.warnings = []

    def warn(self, platform, message):
        text = platform + ': ' + message
        if text not in self.warnings: self.warnings.append(text)

    def add(self, platform, uid, title, location, arguments, artwork=None, ready=True):
        if not isinstance(uid, str) or not re.fullmatch(r'[A-Za-z0-9_.:-]{1,200}', uid):
            raise ValueError('invalid game identifier')
        if not isinstance(title, str) or not title.strip(): raise ValueError('missing game name')
        if not location.is_dir(): return
        key = platform + ':' + uid
        issue = None
        prefix = self.base / self.folders[platform]
        if not (prefix / 'drive_c' / CLIENTS[platform]).is_file():
            issue = 'Install this platform in Platforms, then try again.'
        elif not ready: issue = 'Finish the installation or update in the official launcher.'
        elif not arguments: issue = 'Open the official launcher to play this title.'
        elif platform == 'steam' and uid in STEAM_GPTK_GAMES and self.folders['steam'] == 'steam-bottle':
            issue = ('This game needs Apple GPTK. In Platforms, turn on "Overwatch and Diablo II mode" '
                     'on the Steam card; Steam asks you to sign in once there, and the game shows up again.')
        self.games[key] = dict(id=key, platform=platform, title=title.strip()[:300],
                               install_path=str(location), artwork=artwork,
                               issue=issue, arguments=arguments)

    def steam(self):
        prefix = self.base / self.folders['steam']
        client = prefix / 'drive_c/Program Files (x86)/Steam'
        roots = {client}
        for f in [client / 'steamapps/libraryfolders.vdf', client / 'config/libraryfolders.vdf']:
            if not f.is_file(): continue
            try:
                for key, item in vdf(read_bytes(f).decode('utf-8-sig'))['libraryfolders'].items():
                    if key.isdigit():
                        roots.add(windows_path(prefix, item['path'] if isinstance(item, dict) else item))
            except (OSError, ValueError, KeyError, TypeError, AttributeError):
                self.warn('steam', 'Some library folders could not be read. Open Steam and refresh.')
        for root in sorted(roots):
            if root != client and not root.is_dir():
                self.warn('steam', 'A library drive is unavailable. Reconnect it and refresh.')
            for f in sorted((root / 'steamapps').glob('appmanifest_*.acf')):
                try:
                    item = vdf(read_bytes(f).decode('utf-8-sig'))['AppState']
                    uid = item['appid']
                    if not uid.isdigit() or f.stem != 'appmanifest_' + uid:
                        raise ValueError('mismatched Steam app ID')
                    location = case_path(root / 'steamapps/common', PureWindowsPath(item['installdir']).parts)
                    if PureWindowsPath(item['installdir']).drive: raise ValueError('invalid relative path')
                    artroot = client / 'appcache/librarycache'
                    art = first_file([artroot / uid / 'library_600x900.jpg',
                                      artroot / (uid + '_library_600x900.jpg'),
                                      artroot / uid / 'header.jpg', artroot / (uid + '_header.jpg')])
                    ready = bool(int(item.get('StateFlags', '0')) & 4)
                    self.add('steam', uid, item['name'], location, ['steam://rungameid/' + uid], art, ready)
                except (OSError, ValueError, KeyError, TypeError, AttributeError):
                    self.warn('steam', 'An installation record could not be read. Refresh after Steam finishes updating.')

    def epic(self):
        prefix = self.base / self.folders['epic']
        manifests = prefix / 'drive_c/ProgramData/Epic/EpicGamesLauncher/Data/Manifests'
        for f in sorted(manifests.glob('*.item')):
            try:
                item = read_json(f)
                uid = item['AppName']
                if item.get('MainGameAppName') not in (None, '', uid): continue  # DLC
                location = windows_path(prefix, item['InstallLocation'])
                namespace, catalog = item.get('CatalogNamespace'), item.get('CatalogItemId')
                parts = [namespace, catalog, uid] if namespace and catalog else [uid]
                if any(not isinstance(p, str) for p in parts): raise ValueError('invalid Epic ID')
                uri = 'com.epicgames.launcher://apps/' + quote(':'.join(parts), safe='')
                uri += '?action=launch&silent=true'
                art = first_file(sorted(location.glob('*.ico')))
                ready = not item.get('bIsIncompleteInstall', False)
                executable = item.get('LaunchExecutable')
                if executable:
                    relative = PureWindowsPath(executable)
                    if relative.drive or relative.root: raise ValueError('invalid executable path')
                    exe = case_path(location, relative.parts)
                    ready = ready and exe.is_file()
                    art = art or executable_icon(exe, self.base / 'library-artwork')
                self.add('epic', uid, item['DisplayName'], location, [uri], art, ready)
            except (OSError, ValueError, KeyError, TypeError, AttributeError):
                self.warn('epic', 'An installation record could not be read. Refresh after Epic finishes updating.')

    def battlenet(self):
        prefix = self.base / self.folders['battlenet']
        path = prefix / 'drive_c/ProgramData/Battle.net/Agent/product.db'
        if not path.is_file(): return
        records = protobuf(read_bytes(path)).get(1, [])
        for record in records:
            try:
                fields = protobuf(record)
                uid = fields[1][0].decode('utf-8')
                if uid in ('agent', 'battle.net', 'bna'): continue
                settings = protobuf(fields[3][0])
                location = windows_path(prefix, settings[1][0].decode('utf-8'))
                # An Agent record can outlive an uninstall or partial download.
                if not (location / '.build.info').is_file(): continue
                title, code = BNET.get(uid.casefold(), (location.name, None))
                arguments = ['--exec=launch ' + code] if code else []
                art = first_file(sorted(location.glob('*.ico')))
                if not art:
                    executables = [p for p in location.glob('*.exe') if not any(word in p.name.lower() for word in ('launcher', 'uninstall', 'error', 'crash'))]
                    executables.sort(key=lambda p: p.stat().st_size, reverse=True)
                    for exe in executables[:4]:
                        art = executable_icon(exe, self.base / 'library-artwork')
                        if art: break
                self.add('battlenet', uid, title, location, arguments, art)
            except (OSError, ValueError, KeyError, IndexError, TypeError, AttributeError):
                self.warn('battlenet', 'An installation record could not be read. Refresh after Battle.net finishes updating.')

    def gog(self):
        prefix = self.base / self.folders['gog']
        cdrive = prefix / 'drive_c'
        entries = {}
        database = cdrive / 'ProgramData/GOG.com/Galaxy/storage/galaxy-2.0.db'
        if database.is_file():
            try:
                db = sqlite3.connect(database.as_uri() + '?mode=ro', uri=True, timeout=1)
                try:
                    db.execute('PRAGMA query_only=ON')
                    rows = db.execute('SELECT i.productId, i.installationPath, p.name FROM InstalledBaseProducts i LEFT JOIN Products p ON p.id = i.productId').fetchall()
                    for uid, path, name in rows:
                        entries[str(uid)] = (windows_path(prefix, path), path, name)
                finally: db.close()
            except (sqlite3.Error, OSError, ValueError):
                self.warn('gog', 'Galaxy installation records are unavailable; checking standard game folders.')
        for pattern in ['GOG Games/*/goggame-*.info', 'Program Files/GOG Galaxy/Games/*/goggame-*.info',
                        'Program Files (x86)/GOG Galaxy/Games/*/goggame-*.info']:
            for f in cdrive.glob(pattern):
                uid = f.stem.removeprefix('goggame-')
                path = 'C:\\' + str(f.parent.relative_to(cdrive)).replace('/', '\\')
                entries.setdefault(uid, (f.parent, path, f.parent.name))
        for uid, (location, winpath, title) in entries.items():
            try:
                if not uid.isdigit(): raise ValueError('invalid GOG ID')
                info = location / ('goggame-' + uid + '.info')
                if not info.is_file(): continue
                item = read_json(info)
                if str(item.get('gameId')) != uid: raise ValueError('mismatched GOG ID')
                if str(item.get('rootGameId', uid)) != uid: continue  # DLC
                title = item.get('name') or title or location.name
                art = first_file([location / ('goggame-' + uid + '.ico')])
                args = ['/command=runGame', '/gameId=' + uid, '/path=' + winpath]
                self.add('gog', uid, title, location, args, art)
            except (OSError, ValueError, KeyError, TypeError):
                self.warn('gog', 'An installation record could not be read. Verify this game in Galaxy.')

    def xbox(self):
        # Downloads may be on the Mac or on external drives (xbox_locations.py); a game whose
        # drive is unplugged stays listed as downloaded but not available.
        locations = xbox_locations
        locations.BASE, locations.DEFAULT = self.base, self.base / self.folders['xbox'] / 'drive_c/XboxGames'
        locations.STATE = self.base / 'xbox-locations.json'
        notes = []
        found = locations.scan(warnings=notes)
        for note in notes: self.warn('xbox', note)
        index = locations.load()['games']
        for uid, (location, kind) in sorted(found.items()):
            try:
                art = first_file([self.base / 'library-artwork' / ('xbox-' + uid + '.jpg')])
                if kind == 'missing':
                    title = index.get(uid, {}).get('title') or uid
                    issue = locations.not_connected(location)
                    self.games['xbox:' + uid] = dict(id='xbox:' + uid, platform='xbox', title=title.strip()[:300],
                                                     install_path=str(location / uid), artwork=art, issue=issue,
                                                     arguments=['run', uid], downloaded=True, available=False,
                                                     volume=locations.volume_name(location))
                    continue
                if kind == 'complete':
                    item = read_json(location / '.soju-xbox-install.json')
                    if item.get('schema') != 1 or item.get('product_id') != uid: raise ValueError('mismatched Xbox ID')
                    title, ready = item.get('title') or uid, True
                else:
                    title, ready = uid, False
                self.add('xbox', uid, title, location, ['run', uid], art, ready)
                game = self.games['xbox:' + uid]
                if not ready:
                    game['issue'] = 'This download is not finished. Download again to resume it.'
                elif game['issue'] is None:
                    # The prefix itself is prepared on the first Play; the engine must exist.
                    engine = self.base / 'cx26-engine'
                    if not os.access(engine / 'bin/wine', os.X_OK): game['issue'] = XBOX_ENGINE
                    elif not (engine / 'lib/external/libd3dshared.dylib').is_file(): game['issue'] = XBOX_GPTK
                game['downloaded'] = ready
                game['available'] = True
                game['volume'] = locations.volume_name(location.parent)
            except (OSError, ValueError, KeyError, TypeError, AttributeError):
                self.warn('xbox', 'A download record could not be read. Download the game again to repair it.')

    def didim(self):
        # Native Mac games from Didim Labs: no Windows client or engine involved.
        try: found = didim_games.installed(self.base)
        except (OSError, ValueError, KeyError, TypeError):
            self.warn('didim', 'A Didim Labs game record could not be read. Install the game again.'); return
        for item in found:
            key = 'didim:' + item['uid']
            self.games[key] = dict(id=key, platform='didim', title=item['title'].strip()[:300],
                                   install_path=item['folder'], artwork=item['artwork'], issue=None,
                                   arguments=[item['app']])

    def scan(self):
        self.games = {}; self.warnings = []
        for platform in FOLDERS:
            try: getattr(self, platform)()
            except (OSError, ValueError, KeyError, TypeError, sqlite3.Error):
                self.warn(platform, 'Installation records are temporarily unavailable. Open the launcher and refresh.')
        self.didim()
        accounts, owned, warnings = saved_accounts(self.base)
        self.warnings.extend(warnings)
        for game in owned:
            if game['id'] in self.games:
                self.games[game['id']]['owned'] = True
                # A partial download shows under its Store name.
                if self.games[game['id']]['title'] == game['id'].split(':', 1)[1]:
                    self.games[game['id']]['title'] = game['title']
                if game.get('artwork') and not self.games[game['id']]['artwork']:
                    self.games[game['id']]['artwork'] = game['artwork']
            else:
                self.games[game['id']] = dict(game, arguments=[])
        return {'accounts': accounts, 'games': [{k: v for k, v in g.items() if k != 'arguments'}
                          for g in sorted(self.games.values(), key=lambda g: (g['title'].casefold(), g['id']))],
                'warnings': self.warnings, 'xbox_location': self.xbox_location()}

    def xbox_location(self):
        try: return xbox_locations.describe()
        except (OSError, ValueError, KeyError, TypeError): return None

    def command(self, game_id, install=False):
        self.scan()
        game = self.games.get(game_id)
        if game is None: raise ValueError('This game is no longer installed. Refresh the library.')
        root = Path(__file__).resolve().parent
        if game['platform'] == 'didim':
            if install: raise ValueError('This game is installed. Use Play instead.')
            # -W keeps Soju's task alive until the game quits, so Stop and Running work.
            return ['/usr/bin/open', '-W', '-n', game['arguments'][0]]
        if game['platform'] == 'xbox':
            # A game on an unplugged drive is never downloaded again onto the Mac.
            if game.get('available') is False: raise ValueError(game['issue'])
            if install:
                if game.get('downloaded'): raise ValueError('This game is already downloaded. Use Play instead.')
                if game.get('owned') is not True and not game['install_path']: raise ValueError('This game is not in your Xbox library.')
                if game['issue'] and not game['install_path']: raise ValueError(game['issue'])
                if not (self.base / self.folders['xbox'] / 'drive_c' / CLIENTS['xbox']).is_file():
                    raise ValueError('Set up Xbox PC in Platforms first.')
                return ['/bin/bash', str(root / 'xbox.sh'), 'download', game_id.split(':', 1)[1]]
            if not game.get('downloaded'): raise ValueError(game['issue'] or 'This game is owned but not downloaded. Download it first.')
            if game['issue']: raise ValueError(game['issue'])
            return ['/bin/bash', str(root / 'xbox.sh')] + game['arguments']
        if install:
            if game['install_path']: raise ValueError('This game is installed. Use Play instead.')
            client = self.base / self.folders[game['platform']] / 'drive_c' / CLIENTS[game['platform']]
            if not client.is_file(): raise ValueError('Install this launcher in Platforms first.')
            args = ['steam://install/' + game_id.split(':', 1)[1]] if game['platform'] == 'steam' else []
            return ['/bin/bash', str(Path(__file__).resolve().parent / 'play.sh'), game['platform']] + args
        if not game['install_path']: raise ValueError('This game is owned but not installed. Open its launcher to install it.')
        if game['issue']: raise ValueError(game['issue'])
        return ['/bin/bash', str(root / 'play.sh'), game['platform']] + game['arguments']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['list', 'launch', 'diagnose', 'install'], nargs='?', default='list')
    parser.add_argument('game_id', nargs='?')
    parser.add_argument('--base', default=os.environ.get('SOJU_BASE', str(Path.home() / '.battlenet-macos')))
    parser.add_argument('--dry-run', action='store_true', help='Print launch argv without starting anything')
    args = parser.parse_args()
    lib = Library(args.base)
    try:
        if args.action == 'list':
            print(json.dumps(lib.scan(), ensure_ascii=False)); return
        if not args.game_id: parser.error('choose a game ID from the list')
        if args.action == 'diagnose':
            lib.scan()
            game = lib.games.get(args.game_id)
            if not game: raise ValueError('This game is no longer installed. Refresh the library.')
            print(game['title'] + ' (' + game['platform'] + ')', flush=True)
            print(game['issue'] or 'Installation found. Checking the platform environment…', flush=True)
            if game['platform'] == 'didim':
                print('Native Mac game. Files: ' + game['install_path'], flush=True); return
            command = ['/bin/bash', str(Path(__file__).resolve().parent / 'doctor.sh'), game['platform']]
        else: command = lib.command(args.game_id, install=args.action == 'install')
        if args.dry_run: print(json.dumps(command)); return
        env = dict(os.environ, SOJU_BASE=str(lib.base))
        for key in ['WINEPREFIX', 'ENGINE']: env.pop(key, None)
        os.execve(command[0], command, env)
    except (OSError, ValueError) as error:
        print('soju: ' + str(error), file=sys.stderr); sys.exit(1)


if __name__ == '__main__':
    main()
