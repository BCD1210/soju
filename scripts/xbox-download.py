#!/usr/bin/env python3
"""Download and extract one owned Xbox PC game with the Xodus backend.

Resumable: Xodus keeps a checkpoint in the game folder and continues from it.
The Soju install record is written only after Xodus reports a complete download.
Backend progress records are shown as one short line; URLs and JSON are not printed.
"""
import datetime
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import xbox_locations as locations  # noqa: E402

BASE = Path(os.environ.get('SOJU_BASE', str(Path.home() / '.battlenet-macos')))
CLI = os.environ.get('SOJU_XODUS_BIN') or str(BASE / 'xodus/bin/xodus-cli')
MARKETS = ('US', 'KR', 'GB', 'DE', 'JP')
MARKER = '.soju-xbox-install.json'
NO_XBOX_PROFILE = ("This Microsoft account has no Xbox profile yet. Sign in once at xbox.com and "
                   "choose a gamertag, then download again.")


class DownloadError(Exception):
    pass


def environment():
    env = {k: v for k, v in os.environ.items() if not k.startswith(('DYLD_', 'WINE', 'CX_'))}
    env['XODUS_LOG'] = 'error'
    return env


def product_id(value):
    value = str(value).removeprefix('xbox:').upper()
    if not re.fullmatch(r'[A-Z0-9]{12}', value):
        raise DownloadError('Invalid Microsoft Store product ID.')
    return value


def imported():
    try:
        value = json.loads((BASE / 'account-libraries/xbox.json').read_text())
        return value if isinstance(value, dict) else {}
    except (OSError, ValueError):
        return {}


def market():
    value = os.environ.get('SOJU_XBOX_MARKET', '') or str(imported().get('market', ''))
    return value if value in MARKETS else 'US'


def library_title(pid):
    """The name the library shows (an edition), not the package inside it."""
    for game in imported().get('games') or []:
        if isinstance(game, dict) and game.get('id') == 'xbox:' + pid and isinstance(game.get('title'), str):
            return game['title'].strip()[:300]
    return ''


def choose(plan, pid):
    """Pick the one Windows PC package Xodus can install, or explain why not."""
    if not isinstance(plan, dict) or plan.get('schema') != 1 or not isinstance(plan.get('choices'), list):
        raise DownloadError('The Microsoft Store returned unexpected package data. Please retry later.')
    choices = [c for c in plan['choices'] if isinstance(c, dict)]
    usable = [c for c in choices if c.get('supported') is True and c.get('platform') == 'Windows.Desktop'
              and re.fullmatch(r'[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}', str(c.get('contentId', '')))]
    # An edition bundles the game with add-ons: the game package is the one to install.
    # An add-on is never picked as "the game".
    games = [c for c in usable if c.get('kind') == 'Game']
    if plan.get('status') in ('ready', 'choice') and len(games) == 1:
        return games[0]
    if plan.get('status') == 'choice' and len(games) > 1:
        names = sorted({str(c.get('title', ''))[:80] for c in games})
        raise DownloadError('This Store offer contains several games (' + ', '.join(names) +
                            '). Download each one from its own entry in your library.')
    if any(c.get('packageFormat') == 'MSIXVC2' for c in choices):
        raise DownloadError("This game uses a newer package format (MSIXVC2) that Soju can't download yet.")
    raise DownloadError('No Windows PC package for ' + pid + ' can be downloaded from the Microsoft Store.')


def resolve(pid, region):
    try:
        result = subprocess.run([CLI, 'resolve-package', pid, '--json', '--market', region], env=environment(),
                                stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.SubprocessError):
        raise DownloadError('Could not reach the Microsoft Store. Check your connection and retry.') from None
    if result.returncode != 0:
        raise DownloadError('The Microsoft Store has no package information for this game right now. Please retry later.')
    try:
        return json.loads(result.stdout)
    except ValueError:
        raise DownloadError('The Microsoft Store returned unexpected package data. Please retry later.') from None


def size(value):
    value = float(value)
    for unit in ('B', 'KB', 'MB', 'GB', 'TB'):
        if value < 1000 or unit == 'TB':
            return (format(value, '.0f') if unit in ('B', 'KB') else format(value, '.1f')) + ' ' + unit
        value /= 1000


def clean(line):
    """Backend messages may carry request URLs (with query tokens); keep the words only."""
    line = re.sub(r'[a-z][a-z0-9+.-]*://\S+', '<url>', line.strip())
    return re.sub(r'\S+@\S+', '<account>', line)[:300]


def describe(record):
    phase = record.get('phase')
    done, total, rate = record.get('completedBytes'), record.get('totalBytes'), record.get('bytesPerSecond')
    if phase == 'downloading' and isinstance(done, int) and isinstance(total, int) and total > 0:
        line = '  ' + str(min(100, done * 100 // total)) + '% · ' + size(done) + ' / ' + size(total)
        if isinstance(rate, int) and rate > 0:
            line += ' · ' + size(rate) + '/s'
        return line + (' (paused)' if record.get('paused') else '')
    return {'initializing': '  Preparing the download…', 'metadata': '  Reading the package…',
            'finalizing': '  Finishing the files…', 'complete': '  Download complete.',
            'failed': '  The download stopped.'}.get(phase)


def stream(pid, choice, region, dest):
    args = [CLI, 'streaming', pid, str(dest), '--selected-content-id', choice['contentId'],
            '--market', region, '--progress-json']
    # stderr joins stdout so every backend line goes through clean().
    process = subprocess.Popen(args, env=environment(), stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                               stderr=subprocess.STDOUT, text=True, bufsize=1, errors='replace')
    phase, shown, busy, auth, space = None, None, False, False, None
    try:
        for line in process.stdout:
            if line.startswith('NEXUS_PROGRESS '):
                try:
                    record = json.loads(line[15:])
                except ValueError:
                    continue
                if not isinstance(record, dict) or record.get('schema') != 1:
                    continue
                phase = record.get('phase')
                text = describe(record)
                # Whole percent steps keep the log short on a 100 GB download.
                key = text.split('·')[0] if text else None
                if text and key != shown:
                    print(text, flush=True)
                    shown = key
            elif line.strip():
                busy = busy or 'another download is using this folder' in line
                auth = auth or 'Xbox authentication request failed' in line
                short = re.search(r'not enough free disk space.*?need (\d+) bytes, have (\d+) bytes', line)
                if short:
                    space = max(0, int(short.group(1)) - int(short.group(2)))
                    continue   # reported below in words, without the folder path
                print('  ' + clean(line), flush=True)
    finally:
        # Stopping Soju's download also stops the backend; its checkpoint stays.
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.kill()
        code = process.wait()
    if busy:
        raise DownloadError('This game is already downloading. Pause that download first, or wait for it to finish.')
    if space is not None and code != 0:
        raise DownloadError('Not enough free space on ' + locations.volume_name(dest) + ': this game needs about ' +
                            size(space) + ' more. Free up space or choose another download location in Accounts, '
                            'then download again to resume.')
    if auth and code != 0 and not has_xbox_profile():
        raise DownloadError(NO_XBOX_PROFILE)
    return code == 0 and phase == 'complete'


def has_xbox_profile():
    """False only when Xbox Live has no user for this account (xodus-cli: '(xsts)')."""
    try:
        result = subprocess.run([CLI, 'profile', '--json'], env=environment(), stdin=subprocess.DEVNULL,
                                capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.SubprocessError):
        return True
    return not (result.returncode != 0 and 'unavailable (xsts)' in (result.stderr or ''))


def stop(signum, frame):
    raise KeyboardInterrupt


def main():
    if len(sys.argv) != 2:
        print('Usage: xbox-download.py <ProductId>', file=sys.stderr)
        return 64
    pid_file = None
    signal.signal(signal.SIGTERM, stop)
    try:
        # xbox.sh kill signals only the pids recorded in run/ (no command-line matching).
        run_dir = BASE / 'run'
        run_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        os.chmod(run_dir, 0o700)
        pid_file = run_dir / ('xbox-dl-' + str(os.getpid()) + '.pid')
        pid_file.write_text(str(os.getpid()))
        pid_file.chmod(0o600)
        pid = product_id(sys.argv[1])
        if not os.access(CLI, os.X_OK):
            raise DownloadError('Set up Xbox PC in Platforms first (or run: soju xbox-install).')
        region = market()
        choice = choose(resolve(pid, region), pid)
        dest = locations.destination(pid)
        dest.mkdir(parents=True, exist_ok=True)
        if not locations.inside(dest, dest.parent) or dest.is_symlink():
            raise DownloadError('The game folder is a link. Remove it and download again.')
        title = library_title(pid) or str(choice.get('title') or pid).strip()[:300]
        print('Downloading ' + title + ' (' + str(choice.get('packageFormat')) + ') to ' + locations.tilde(dest), flush=True)
        if not stream(pid, choice, region, dest):
            raise DownloadError('The download did not finish. Download again to resume it.')
        record = dict(schema=1, product_id=pid, content_id=choice['contentId'], title=title,
                      installed_at=datetime.datetime.now(datetime.timezone.utc).isoformat())
        (dest / MARKER).write_text(json.dumps(record, ensure_ascii=False) + '\n')
        try:
            locations.remember(pid, dest, title)
        except OSError:
            pass   # the next library scan records it; the download itself is complete
        print(title + ' is downloaded. Press Play in Soju to start it.')
        return 0
    except (DownloadError, locations.LocationError, OSError) as error:
        print('soju: ' + str(error), file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        print('soju: Download stopped. Download again to resume it.', file=sys.stderr)
        return 130
    finally:
        if pid_file:
            pid_file.unlink(missing_ok=True)


if __name__ == '__main__':
    sys.exit(main())
