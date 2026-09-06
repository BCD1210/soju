"""Public store search and locally saved account libraries. Python 3.9+."""
import concurrent.futures
import datetime
import json
import os
from pathlib import Path
import re
import sqlite3
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

LIMIT = 8 * 1024 * 1024
REGIONS = {'US': 'USD', 'KR': 'KRW', 'GB': 'GBP', 'DE': 'EUR', 'JP': 'JPY'}
STORES = {'steam': 'https://store.steampowered.com', 'gog': 'https://www.gog.com'}


class ServiceError(ValueError):
    pass


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise ServiceError('The service redirected the request. Please retry later.')


def get_json(url):
    # Keep provider errors concise; never log response bodies or request details.
    try:
        request = urllib.request.Request(url, headers={'User-Agent': 'Soju/1.6 (+https://github.com/BCD1210/soju)', 'Accept': 'application/json'})
        with urllib.request.build_opener(NoRedirect).open(request, timeout=15) as response:
            data = response.read(LIMIT + 1)
        if len(data) > LIMIT:
            raise ServiceError('The service returned too much data.')
        return json.loads(data)
    except urllib.error.HTTPError as error:
        if error.code in (401, 403):
            raise ServiceError('Access was not granted. Check your account settings or use the official store.') from None
        if error.code == 429:
            raise ServiceError('Too many requests. Please retry in a few minutes.') from None
        raise ServiceError('The service is temporarily unavailable.') from None
    except (OSError, ValueError) as error:
        if isinstance(error, ServiceError):
            raise
        raise ServiceError('Could not read the service response. Check your connection and retry.') from None


def text(value, limit=300):
    return value.strip()[:limit] if isinstance(value, str) else ''


def numeric_id(value):
    value = str(value)
    if not re.fullmatch(r'[0-9]{1,20}', value) or int(value) == 0:
        raise ServiceError('Invalid game identifier.')
    return value


def safe_image(value, platform):
    if not isinstance(value, str):
        return None
    parsed = urllib.parse.urlsplit(value)
    allowed = {'steam': ('steamstatic.com', 'steamcdn-a.akamaihd.net'),
               'gog': ('gog-statics.com', 'gog.com')}
    host = (parsed.hostname or '').lower()
    if parsed.scheme != 'https' or parsed.username or parsed.password or parsed.port not in (None, 443):
        return None
    return value if any(host == h or host.endswith('.' + h) for h in allowed[platform]) else None


def store_url(platform, uid, slug=''):
    if platform == 'steam':
        return STORES[platform] + '/app/' + numeric_id(uid) + '/'
    if slug and re.fullmatch(r'[a-zA-Z0-9_-]{1,200}', slug):
        return STORES['gog'] + '/en/game/' + slug
    return STORES['gog'] + '/game/' + numeric_id(uid)


def steam_search(query, country):
    url = STORES['steam'] + '/api/storesearch/?' + urllib.parse.urlencode({'term': query, 'l': 'english', 'cc': country})
    data = get_json(url)
    if not isinstance(data, dict) or not isinstance(data.get('items'), list):
        raise ServiceError('Steam search is unavailable.')
    result = []
    for item in data['items'][:24]:
        try:
            uid = numeric_id(item['id'])
            if item.get('type') != 'app' or not text(item.get('name')):
                continue
            price = item.get('price') or {}
            amount, currency = price.get('final'), price.get('currency')
            label = 'View price in store'
            if isinstance(amount, int) and amount >= 0 and re.fullmatch(r'[A-Z]{3}', currency or ''):
                label = 'Free' if amount == 0 else currency + ' ' + format(amount / 100, ',.2f')
            result.append(dict(id='steam:' + uid, platform='steam', title=text(item['name']),
                               url=store_url('steam', uid), artwork=safe_image(item.get('tiny_image'), 'steam'),
                               price=label, mac=bool(item.get('platforms', {}).get('mac')), windows=bool(item.get('platforms', {}).get('windows'))))
        except (KeyError, TypeError, ValueError, AttributeError):
            continue
    return result


def gog_search(query, country):
    url = 'https://catalog.gog.com/v1/catalog?' + urllib.parse.urlencode({
        'query': query, 'limit': 24, 'locale': 'en-US', 'countryCode': country,
        'currencyCode': REGIONS[country], 'productType': 'in:game,pack'})
    data = get_json(url)
    if not isinstance(data, dict) or not isinstance(data.get('products'), list):
        raise ServiceError('GOG search is unavailable.')
    result = []
    for item in data['products'][:24]:
        try:
            uid = numeric_id(item['id'])
            if not text(item.get('title')):
                continue
            price = item.get('price') or {}
            money = price.get('finalMoney') or {}
            label = 'View price in store'
            if re.fullmatch(r'[0-9]+(?:\.[0-9]+)?', str(money.get('amount', ''))) and re.fullmatch(r'[A-Z]{3}', money.get('currency', '')):
                label = 'Free' if float(money['amount']) == 0 else money['currency'] + ' ' + str(money['amount'])
            systems = item.get('operatingSystems') or []
            result.append(dict(id='gog:' + uid, platform='gog', title=text(item['title']),
                               url=store_url('gog', uid, item.get('slug', '')),
                               artwork=safe_image(item.get('coverHorizontal'), 'gog'), price=label,
                               mac='osx' in systems or 'mac' in systems, windows='windows' in systems))
        except (KeyError, TypeError, ValueError, AttributeError):
            continue
    return result


def search(query, country):
    query = text(query, 100)
    if len(query) < 2:
        raise ServiceError('Enter at least two characters.')
    if country not in REGIONS:
        raise ServiceError('Choose a supported store region.')
    games, warnings = [], []
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        jobs = {pool.submit(fn, query, country): name for name, fn in [('steam', steam_search), ('gog', gog_search)]}
        for job in concurrent.futures.as_completed(jobs):
            try:
                games.extend(job.result())
            except Exception:
                warnings.append(jobs[job].title() + ' search is unavailable. Please retry later.')
    unique = {g['id']: g for g in games}
    def rank(game):
        title, term = game['title'].casefold(), query.casefold()
        score = 0 if title == term else 1 if title.startswith(term) else 2 if term in title else 3 if all(word in title for word in term.split()) else 4
        return score, title, game['platform']
    return dict(games=sorted(unique.values(), key=rank), warnings=sorted(warnings), country=country)


def steam_owned(settings):
    """Validate the metadata-only snapshot returned by Steam's signed-in web page."""
    try:
        value = json.loads(settings.get('snapshot', ''))
    except (ValueError, TypeError):
        raise ServiceError('Sign in to Steam in Accounts to import your library.') from None
    if not isinstance(value, dict) or value.get('state') != 'ready':
        raise ServiceError('Steam did not share a complete library. Your previous import is unchanged.')
    profile = value.get('steamid')
    if not isinstance(profile, str) or not re.fullmatch(r'765[0-9]{14}', profile):
        raise ServiceError('Steam could not confirm the signed-in account.')
    source, count = value.get('games'), value.get('game_count')
    if not isinstance(source, list) or type(count) is not int or not 0 <= count <= 20000 or len(source) != count:
        raise ServiceError('Steam returned an incomplete library. Please retry.')
    games, seen = [], set()
    for item in source:
        if not isinstance(item, dict):
            raise ServiceError('Steam returned invalid game metadata.')
        uid, title = numeric_id(item.get('appid')), text(item.get('name'))
        if not title or uid in seen:
            raise ServiceError('Steam returned incomplete game names. Please retry.')
        seen.add(uid)
        games.append(dict(id='steam:' + uid, platform='steam', title=title))
    return games, 'Steam · …' + profile[-4:]


def gog_database(base):
    path = Path(base) / 'gog-bottle/drive_c/ProgramData/GOG.com/Galaxy/storage/galaxy-2.0.db'
    if not path.is_file():
        raise ServiceError('Open GOG GALAXY, sign in and let your library load, then retry.')
    db = sqlite3.connect(path.absolute().as_uri() + '?mode=ro', uri=True, timeout=1)
    db.execute('PRAGMA query_only=ON')
    deadline = time.monotonic() + 4
    db.set_progress_handler(lambda: int(time.monotonic() > deadline), 10000)
    return db


def gog_users(base):
    db = gog_database(base)
    try:
        rows = db.execute("""SELECT r.userId, COUNT(*) FROM LibraryReleases r
            JOIN LicensedReleases l ON l.libraryId=r.id
            WHERE l.isOwned=1 AND r.releaseKey GLOB 'gog_[0-9]*'
            GROUP BY r.userId ORDER BY r.userId""").fetchall()
        return [dict(id=str(uid), label='Galaxy · …' + str(uid)[-4:], count=count) for uid, count in rows]
    finally:
        db.close()


def gog_owned(base, settings):
    users = gog_users(base)
    selected = settings.get('user_id') or (users[0]['id'] if len(users) == 1 else None)
    if not selected or selected not in {u['id'] for u in users}:
        raise ServiceError('Select a Galaxy account. If none appear, open Galaxy and let your owned library load.')
    db = gog_database(base)
    try:
        # releaseKey is a platform/game identifier. No auth, license-key or token columns are read.
        rows = db.execute("""SELECT r.releaseKey, p.name, gp.value FROM LibraryReleases r
            JOIN LicensedReleases l ON l.libraryId=r.id AND l.isOwned=1
            LEFT JOIN ReleaseProperties rp ON rp.releaseKey=r.releaseKey
            LEFT JOIN ProductsToReleaseKeys pr ON pr.releaseKey=r.releaseKey
            LEFT JOIN Products p ON p.id=pr.gogId
            LEFT JOIN GamePieces gp ON gp.releaseKey=r.releaseKey
              AND gp.gamePieceTypeId=(SELECT id FROM GamePieceTypes WHERE type='originalTitle')
            WHERE r.userId=? AND r.releaseKey GLOB 'gog_[0-9]*'
              AND COALESCE(rp.isDlc,0)=0 LIMIT 40001""", (selected,)).fetchall()
    finally:
        db.close()
    if len(rows) > 40000:
        raise ServiceError('Galaxy returned too much metadata.')
    games = {}
    for release, name, value in rows:
        uid = numeric_id(release.removeprefix('gog_'))
        title = text(name)
        if not title and value:
            parsed = json.loads(value)
            title = text(parsed.get('title')) if isinstance(parsed, dict) else text(parsed)
        if not title:
            raise ServiceError('Galaxy is still loading game names. Open its library and retry. Your previous import is unchanged.')
        games[uid] = dict(id='gog:' + uid, platform='gog', title=title)
    return list(games.values()), 'Galaxy · …' + selected[-4:]


def account_path(base, platform):
    if platform not in ('steam', 'gog'):
        raise ServiceError('Unsupported account provider.')
    return Path(base) / 'account-libraries' / (platform + '.json')


def save_account(base, platform, games, label):
    path = account_path(base, platform)
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    payload = dict(schema=1, platform=platform, label=label,
                   updated_at=datetime.datetime.now(datetime.timezone.utc).isoformat(), games=games)
    data = json.dumps(payload, ensure_ascii=False).encode()
    if len(data) > LIMIT:
        raise ServiceError('This library exceeds the supported import size.')
    fd, temporary = tempfile.mkstemp(prefix='.' + platform, dir=path.parent)
    try:
        with os.fdopen(fd, 'wb') as stream:
            stream.write(data)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)
    return dict(message='Imported ' + str(len(games)) + ' games.', count=len(games))


def saved_accounts(base):
    accounts, games, warnings = [], [], []
    for platform in ('steam', 'gog'):
        path = account_path(base, platform)
        if not path.is_file():
            continue
        try:
            with path.open('rb') as stream:
                data = stream.read(LIMIT + 1)
            if len(data) > LIMIT:
                raise ValueError()
            data = json.loads(data)
            if data.get('schema') != 1 or data.get('platform') != platform or not isinstance(data.get('games'), list):
                raise ValueError()
            imported = {}
            for game in data['games']:
                uid = numeric_id(game['id'].removeprefix(platform + ':'))
                if game['id'] != platform + ':' + uid or not text(game.get('title')):
                    raise ValueError()
                imported[game['id']] = dict(id=game['id'], platform=platform, title=text(game['title']),
                                           install_path='', artwork=None, issue=None, owned=True)
            games.extend(imported.values())
            accounts.append(dict(platform=platform, label=text(data.get('label')), updated_at=text(data.get('updated_at')), count=len(imported)))
        except (OSError, ValueError, TypeError, KeyError, AttributeError):
            warnings.append(platform.title() + ' account import could not be read. Reconnect it in Accounts.')
    return accounts, games, warnings
