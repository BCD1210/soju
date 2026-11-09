#!/usr/bin/env python3
"""Play Soju games on a TV, iPad or phone: Soju sets up Sunshine (open-source game streaming server,
LizardByte) on this Mac, and the other device connects with the free Moonlight app.

  stream_tv.py setup        download the pinned Sunshine release, check it, install, configure, start
  stream_tv.py start|stop   start or stop Sunshine
  stream_tv.py status       JSON: installed, running, permission, devices, pending
  stream_tv.py pin 1234     approve the device that is showing this PIN in Moonlight
  stream_tv.py unpair NAME  remove a paired device

Sunshine has no telemetry; its settings page only checks GitHub for new versions. Soju keeps the
web settings reachable from this Mac only and turns UPnP off, so nothing is opened to the internet.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import ssl
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

VERSION = 'v2026.914.233613'
DMG_URL = 'https://github.com/LizardByte/Sunshine/releases/download/' + VERSION + '/Sunshine-macOS-arm64.dmg'
DMG_SHA256 = '795f03a389c5726f9a7b15f501dc1ddf204b442cae565859ac83aae027a9e07c'
TEAM_ID = 'F8XQ7XCN2R'  # LizardByte LLC
APP = Path.home() / 'Applications' / 'Sunshine.app'
BINARY = APP / 'Contents' / 'MacOS' / 'Sunshine'
CONFIG_DIR = Path.home() / '.config' / 'sunshine'
LOG = CONFIG_DIR / 'sunshine.log'
API = 'https://localhost:47990/api/'
KEYCHAIN = 'Soju Sunshine'
SETTINGS = {'sunshine_name': 'Soju', 'origin_web_ui_allowed': 'pc', 'upnp': 'disabled'}


def say(text):
    print(text, flush=True)


def running():
    return subprocess.run(['/usr/bin/pgrep', '-f', str(BINARY)], capture_output=True).returncode == 0


def password(create=False):
    found = subprocess.run(['/usr/bin/security', 'find-generic-password', '-s', KEYCHAIN, '-a', 'soju', '-w'],
                           capture_output=True, text=True)
    if found.returncode == 0:
        return found.stdout.strip()
    if not create:
        return ''
    value = secrets.token_urlsafe(18)
    subprocess.run(['/usr/bin/security', 'add-generic-password', '-U', '-s', KEYCHAIN, '-a', 'soju',
                    '-l', 'Soju: Sunshine settings', '-w', value], check=True, capture_output=True)
    return value


def api(path, body=None, method=None):
    """Sunshine's local settings API (self-signed certificate on localhost)."""
    import base64
    request = urllib.request.Request(API + path, method=method or ('POST' if body is not None else 'GET'),
                                     data=json.dumps(body).encode() if body is not None else None)
    request.add_header('Authorization', 'Basic ' + base64.b64encode(('soju:' + password()).encode()).decode())
    request.add_header('Content-Type', 'application/json')
    context = ssl.create_default_context()
    context.check_hostname = False
    context.verify_mode = ssl.CERT_NONE
    with urllib.request.urlopen(request, timeout=10, context=context) as response:
        return json.loads(response.read(1024 * 1024) or b'{}')


def installed_version():
    plist = APP / 'Contents' / 'Info.plist'
    if not plist.exists():
        return ''
    import plistlib
    return plistlib.loads(plist.read_bytes()).get('CFBundleShortVersionString', '')


def install():
    folder = Path(tempfile.mkdtemp(prefix='soju-sunshine-'))
    try:
        dmg = folder / 'Sunshine.dmg'
        say('Downloading Sunshine ' + VERSION + '…')
        request = urllib.request.Request(DMG_URL, headers={'User-Agent': 'Soju'})
        sha = hashlib.sha256()
        with urllib.request.urlopen(request, timeout=60) as response, dmg.open('wb') as out:
            while chunk := response.read(1024 * 1024):
                out.write(chunk); sha.update(chunk)
        if sha.hexdigest() != DMG_SHA256:
            raise ValueError('The Sunshine download did not match the expected checksum.')
        mount = folder / 'mnt'
        subprocess.run(['/usr/bin/hdiutil', 'attach', '-nobrowse', '-readonly', '-mountpoint', str(mount), str(dmg)],
                       check=True, capture_output=True)
        try:
            source = mount / 'Sunshine.app'
            sign = subprocess.run(['/usr/bin/codesign', '-dv', '--verbose=2', str(source)], capture_output=True, text=True)
            if 'TeamIdentifier=' + TEAM_ID not in sign.stderr:
                raise ValueError('Sunshine is not signed by LizardByte.')
            gate = subprocess.run(['/usr/sbin/spctl', '-a', '-vv', str(source)], capture_output=True, text=True)
            if 'Notarized Developer ID' not in gate.stderr:
                raise ValueError('Sunshine is not notarized by Apple.')
            stop()
            APP.parent.mkdir(parents=True, exist_ok=True)
            if APP.exists():
                shutil.rmtree(APP)
            subprocess.run(['/usr/bin/ditto', str(source), str(APP)], check=True)
        finally:
            subprocess.run(['/usr/bin/hdiutil', 'detach', str(mount)], capture_output=True)
    finally:
        shutil.rmtree(folder, ignore_errors=True)


def configure():
    CONFIG_DIR.mkdir(parents=True, exist_ok=True, mode=0o700)
    conf = CONFIG_DIR / 'sunshine.conf'
    lines = [l for l in (conf.read_text().splitlines() if conf.exists() else [])
             if l.split('=')[0].strip() not in SETTINGS]
    lines += [k + ' = ' + v for k, v in SETTINGS.items()]
    conf.write_text('\n'.join(lines) + '\n')
    os.chmod(conf, 0o600)
    if not password():
        subprocess.run([str(BINARY), '--creds', 'soju', password(create=True)], check=True, capture_output=True)


def needs_permission():
    if not LOG.exists():
        return False
    text = LOG.read_text(errors='replace')[-200000:]
    last_start = text.rfind('Info: Sunshine version')
    return 'No screen capture permission' in (text[last_start:] if last_start >= 0 else text)


def start():
    if not APP.exists():
        raise ValueError('Set up streaming first.')
    if running():
        return
    subprocess.run(['/usr/bin/open', '-g', str(APP)], check=True)
    for _ in range(30):
        time.sleep(0.5)
        try:
            api('clients/list'); break
        except (urllib.error.URLError, OSError, ValueError):
            continue
    if needs_permission():
        subprocess.run(['/usr/bin/open', 'x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_ScreenCapture'])
        say('Turn on Sunshine under Screen & System Audio Recording, then press Start again.')


def stop():
    subprocess.run(['/usr/bin/pkill', '-f', str(BINARY)], capture_output=True)


def status():
    result = {'installed': APP.exists(), 'version': installed_version(), 'running': running(),
              'permission': not needs_permission(), 'devices': [], 'pending': 0}
    if result['running'] and password():
        try:
            result['devices'] = [c.get('name', '') for c in api('clients/list').get('named_certs', [])]
            result['pending'] = len(api('pin').get('pairings', []))
        except (urllib.error.URLError, OSError, ValueError):
            pass
    print(json.dumps(result))


def pin(code):
    if not re.fullmatch(r'\d{4}', code or ''):
        raise ValueError('Enter the 4-digit PIN that Moonlight shows.')
    pending = api('pin').get('pairings', [])
    if not pending:
        raise ValueError('No device is waiting. In Moonlight, choose this Mac first, then enter the PIN it shows.')
    request = pending[-1]
    name = re.sub(r'[^\w .-]', '', request.get('name') or 'Moonlight')[:40] or 'Moonlight'
    if not api('pin', {'pairing_id': request['id'], 'pin': code, 'name': name}).get('status'):
        raise ValueError('Sunshine did not accept that PIN.')
    say('Paired ' + name + '.')


def unpair(name):
    for client in api('clients/list').get('named_certs', []):
        if client.get('name') == name:
            api('clients/unpair', {'uuid': client['uuid']})
            say('Removed ' + name + '.')
            return
    raise ValueError('No paired device called ' + name + '.')


def main():
    action = sys.argv[1] if len(sys.argv) > 1 else 'status'
    try:
        if action == 'setup':
            if installed_version() != VERSION.lstrip('v'):
                install()
            configure()
            start()
            say('Streaming is set up.' if running() else 'Sunshine did not start.')
        elif action == 'start': start()
        elif action == 'stop': stop()
        elif action == 'status': status()
        elif action == 'pin': pin(sys.argv[2] if len(sys.argv) > 2 else '')
        elif action == 'unpair': unpair(sys.argv[2] if len(sys.argv) > 2 else '')
        else: raise ValueError('Unknown action.')
    except (OSError, ValueError, subprocess.CalledProcessError, urllib.error.URLError) as error:
        print('soju: ' + str(error), file=sys.stderr); sys.exit(1)


if __name__ == '__main__':
    main()
