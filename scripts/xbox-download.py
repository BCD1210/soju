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

BASE = Path(os.environ.get('SOJU_BASE', str(Path.home() / '.battlenet-macos')))
CLI = os.environ.get('SOJU_XODUS_BIN') or str(BASE / 'xodus/bin/xodus-cli')
GAMES = BASE / 'xbox-bottle/drive_c/XboxGames'
MARKETS = ('US', 'KR', 'GB', 'DE', 'JP')
MARKER = '.soju-xbox-install.json'


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
    if plan.get('status') == 'ready' and len(usable) == 1:
        return usable[0]
    if plan.get('status') == 'choice' and len(usable) == 1:
        return usable[0]
    if plan.get('status') == 'choice' and usable:
        names = sorted({str(c.get('title', ''))[:80] for c in usable})
        raise DownloadError('This Store offer contains several games (' + ', '.join(names) +
                            '). Download each one from its own entry in your library.')
    if any(c.get('packageFormat') == 'MSIXVC2' for c in choices):
        raise DownloadError('This game uses the MSIXVC2 package format, which the Xodus backend does not support yet (xodus#53).')
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
    process = subprocess.Popen(args, env=environment(), stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                               text=True, bufsize=1)
    phase, shown = None, None
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
                print('  ' + line.strip()[:300], flush=True)
    finally:
        # Stopping Soju's download also stops the backend; its checkpoint stays.
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.kill()
        code = process.wait()
    return code == 0 and phase == 'complete'


def stop(signum, frame):
    raise KeyboardInterrupt


def main():
    if len(sys.argv) != 2:
        print('Usage: xbox-download.py <ProductId>', file=sys.stderr)
        return 64
    signal.signal(signal.SIGTERM, stop)
    try:
        pid = product_id(sys.argv[1])
        if not os.access(CLI, os.X_OK):
            raise DownloadError('Set up Xbox PC in Platforms first (or run: soju xbox-install).')
        region = market()
        choice = choose(resolve(pid, region), pid)
        dest = GAMES / pid
        dest.mkdir(parents=True, exist_ok=True)
        title = library_title(pid) or str(choice.get('title') or pid).strip()[:300]
        print('Downloading ' + title + ' (' + str(choice.get('packageFormat')) + ') to ' + str(dest), flush=True)
        if not stream(pid, choice, region, dest):
            raise DownloadError('The download did not finish. Download again to resume it.')
        record = dict(schema=1, product_id=pid, content_id=choice['contentId'], title=title,
                      installed_at=datetime.datetime.now(datetime.timezone.utc).isoformat())
        (dest / MARKER).write_text(json.dumps(record, ensure_ascii=False) + '\n')
        print(title + ' is downloaded. Launching Xbox PC games is not available in this version of Soju.')
        return 0
    except DownloadError as error:
        print('soju: ' + str(error), file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        print('soju: Download stopped. Download again to resume it.', file=sys.stderr)
        return 130


if __name__ == '__main__':
    sys.exit(main())
