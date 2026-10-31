#!/usr/bin/env python3
"""Start a downloaded Xbox PC game.

1. finds the game's executable in MicrosoftGame.config (or appxmanifest.xml);
2. prepares the Xbox prefix (runtime DLLs, overrides, VC++) when needed;
3. starts xodus-service (account, licensing and cloud saves for the game) on a
   private socket, plus a loopback TCP bridge: macOS Wine has no AF_UNIX
   Winsock, so the game's xgameruntime reaches the service over 127.0.0.1 with
   a random per-launch key (XODUS_BRIDGE_PORT / XODUS_BRIDGE_KEY);
4. runs `xodus-cli run`, which gets the licence, decrypts the game into a
   temporary RAM-disk view and starts it with scripts/xbox-wine.sh. Files the
   game writes there are copied back into its folder when it exits.
Stopping (Ctrl+C or SIGTERM) asks xodus-cli to stop the game so saves sync; if
the game is still running 20 s later, everything in the Xbox prefix is ended.
Only one session per game runs at a time.
"""
import fcntl
import hmac
import json
import os
from pathlib import Path, PurePosixPath, PureWindowsPath
import re
import secrets
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
BASE = Path(os.environ.get('SOJU_BASE', str(Path.home() / '.battlenet-macos')))
CLI = Path(os.environ.get('SOJU_XODUS_BIN') or str(BASE / 'xodus/bin/xodus-cli'))
GAMES = BASE / 'xbox-bottle/drive_c/XboxGames'
MARKETS = ('US', 'KR', 'GB', 'DE', 'JP')


class LaunchError(Exception):
    pass


def product_id(value):
    value = str(value).removeprefix('xbox:').upper()
    if not re.fullmatch(r'[A-Z0-9]{12}', value):
        raise LaunchError('Invalid Microsoft Store product ID.')
    return value


def find(folder, name):
    return next((p for p in folder.iterdir() if p.name.casefold() == name.casefold() and p.is_file()), None)


def local(tag):
    return tag.rsplit('}', 1)[-1].casefold()


def executable(folder):
    """The launch entry the package declares; never a path outside the game folder."""
    for name, element, attribute in (('MicrosoftGame.config', 'executable', 'name'),
                                     ('appxmanifest.xml', 'application', 'executable')):
        path = find(folder, name)
        if not path or path.stat().st_size > 4 * 1024 * 1024:
            continue
        try:
            tree = ET.parse(path)
        except ET.ParseError:
            continue
        for node in tree.iter():
            if local(node.tag) != element:
                continue
            value = next((v for k, v in node.attrib.items() if local(k) == attribute), '').strip()
            parts = PureWindowsPath(value).parts
            if value and not PureWindowsPath(value).drive and not PureWindowsPath(value).root and '..' not in parts:
                return value
    raise LaunchError('The game package does not name its executable (MicrosoftGame.config).')


def market():
    value = os.environ.get('SOJU_XBOX_MARKET', '')
    if not value:
        try:
            value = json.loads((BASE / 'account-libraries/xbox.json').read_text()).get('market', '')
        except (OSError, ValueError, AttributeError):
            value = ''
    return value if value in MARKETS else 'US'


def clean(line):
    """Backend messages may carry request URLs (with query tokens); keep the words only."""
    return re.sub(r'[a-z][a-z0-9+.-]*://\S+', '<url>', line.rstrip())[:500]


class Bridge:
    """127.0.0.1 TCP -> xodus-service socket. A client must send the key first.
    Only a few connections may wait for authentication at once, briefly, so other
    local processes cannot pile up threads during a game session."""
    PENDING = 8

    def __init__(self, socket_path):
        self.pending = threading.BoundedSemaphore(self.PENDING)
        self.socket_path = socket_path
        self.key = secrets.token_hex(32).encode()
        self.server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.server.bind(('127.0.0.1', 0))
        self.server.listen(16)
        self.port = self.server.getsockname()[1]
        self.closed = False
        threading.Thread(target=self.accept, daemon=True).start()

    def accept(self):
        while not self.closed:
            try:
                client, _ = self.server.accept()
            except OSError:
                return
            if not self.pending.acquire(blocking=False):
                client.close()
                continue
            threading.Thread(target=self.handle, args=(client,), daemon=True).start()

    def handle(self, client):
        upstream = None
        waiting = True
        try:
            client.settimeout(2)
            received = b''
            while len(received) < len(self.key):
                chunk = client.recv(len(self.key) - len(received))
                if not chunk:
                    return
                received += chunk
            waiting = False
            self.pending.release()
            if not hmac.compare_digest(received, self.key):
                return
            client.settimeout(None)
            upstream = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            upstream.connect(self.socket_path)
            threading.Thread(target=self.pipe, args=(upstream, client), daemon=True).start()
            self.pipe(client, upstream)
        except OSError:
            pass
        finally:
            if waiting:
                self.pending.release()
            for s in (client, upstream):
                if s:
                    try:
                        s.close()
                    except OSError:
                        pass

    @staticmethod
    def pipe(source, target):
        try:
            while True:
                data = source.recv(65536)
                if not data:
                    break
                target.sendall(data)
        except OSError:
            pass
        finally:
            try:
                target.shutdown(socket.SHUT_WR)
            except OSError:
                pass

    def close(self):
        self.closed = True
        self.server.close()


class Service:
    def __init__(self):
        binary = CLI.parent / 'xodus-service'
        if not os.access(binary, os.X_OK):
            raise LaunchError('The Xodus backend has no xodus-service. Run Update components in Platforms.')
        # macOS limits socket paths to 104 bytes, so the session lives in /tmp.
        self.directory = Path(tempfile.mkdtemp(prefix='soju-xbox-', dir='/tmp'))
        self.socket = str(self.directory / 'xodus.sock')
        env = {'PATH': '/usr/bin:/bin', 'HOME': str(Path.home()), 'XDG_RUNTIME_DIR': str(self.directory), 'XODUS_LOG': 'error'}
        self.process = subprocess.Popen([str(binary)], env=env, stdin=subprocess.DEVNULL,
                                        stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        # Drain stderr for the whole session (a full pipe would block the service);
        # keep only whether it reported a credential problem.
        self.credential_error = False
        threading.Thread(target=self.drain, daemon=True).start()
        self.bridge = None
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                time.sleep(0.2)
                self.stop()
                if self.credential_error:
                    raise LaunchError('Sign in to your Microsoft account in Accounts, then press Play again.')
                raise LaunchError('The Xbox account service stopped. Check your connection and sign-in, then retry.')
            try:
                with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as probe:
                    probe.connect(self.socket)
                self.bridge = Bridge(self.socket)
                return
            except OSError:
                time.sleep(0.25)
        self.stop()
        raise LaunchError('The Xbox account service did not start within 90 seconds.')

    def drain(self):
        for line in self.process.stderr:
            if b'NEXUS_CREDENTIAL_ERROR_V1' in line:
                self.credential_error = True

    def stop(self):
        if self.bridge:
            self.bridge.close()
        # The service holds the account tokens: make sure it is gone.
        for send in (lambda: self.process.send_signal(signal.SIGINT), self.process.terminate, self.process.kill):
            if self.process.poll() is not None:
                break
            send()
            try:
                self.process.wait(timeout=4)
            except subprocess.TimeoutExpired:
                pass
        shutil.rmtree(self.directory, ignore_errors=True)


def main():
    if len(sys.argv) != 2:
        print('Usage: xbox-run.py <ProductId>', file=sys.stderr)
        return 64
    service = game = None
    stop = threading.Event()
    # xbox.sh kill signals only the pids recorded here (no command-line matching).
    run_dir = BASE / 'run'
    run_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
    pid_file = run_dir / ('xbox-' + str(os.getpid()) + '.pid')
    pid_file.write_text(str(os.getpid()))
    pid_file.chmod(0o600)
    def force_stop():
        if not stop.wait() or game is None:
            return
        try:
            game.wait(timeout=20)
        except subprocess.TimeoutExpired:
            subprocess.run(['/bin/sh', str(ROOT / 'scripts/xbox-wine.sh'), '--soju-stop-prefix'],
                           env=dict(os.environ, SOJU_BASE=str(BASE)), capture_output=True)
    def interrupt(signum, frame):
        if stop.is_set():
            return
        stop.set()
        if game and game.poll() is None:
            game.send_signal(signal.SIGINT)
            threading.Thread(target=force_stop, daemon=True).start()
    signal.signal(signal.SIGTERM, interrupt)
    signal.signal(signal.SIGINT, interrupt)
    try:
        pid = product_id(sys.argv[1])
        folder = GAMES / pid
        record = folder / '.soju-xbox-install.json'
        if not record.is_file():
            raise LaunchError('This game is not downloaded. Download it in Soju first.')
        if not (folder / '.xodus-streaming.msixvc').is_file():
            raise LaunchError('The game download is incomplete. Download it again to repair it.')
        lock = (folder / '.soju-xbox-run.lock').open('w')
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError:
            raise LaunchError('This game is already running.') from None
        exe = executable(folder)
        if subprocess.run([sys.executable, str(ROOT / 'scripts/xbox-prefix.py')]).returncode != 0:
            return 1
        title = json.loads(record.read_text()).get('title') or pid
        print('Starting ' + str(title)[:300] + ' (' + exe + ')…', flush=True)
        service = Service()
        if stop.is_set():
            return 130
        env = {k: v for k, v in os.environ.items() if not k.startswith(('DYLD_', 'WINE', 'CX_'))}
        env.update(SOJU_BASE=str(BASE), XODUS_LOG='error', XODUS_SOCKET=service.socket,
                   XODUS_BRIDGE_PORT=str(service.bridge.port), XODUS_BRIDGE_KEY=service.bridge.key.decode(),
                   XODUS_SAVE_SYNC='auto')
        game = subprocess.Popen([str(CLI), 'run', str(folder), str(ROOT / 'scripts/xbox-wine.sh'), '--exe', exe,
                                 '--materialize-chain', '--market', market()],
                                cwd=str(folder), env=env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, text=True, bufsize=1, errors='replace')
        for line in game.stdout:
            if line.strip():
                print(clean(line), flush=True)
        code = game.wait()
        print('The game exited (code ' + str(code) + ').', flush=True)
        return 0 if code == 0 or stop.is_set() else code
    except LaunchError as error:
        print('soju: ' + str(error), file=sys.stderr)
        return 1
    finally:
        if service:
            service.stop()
        pid_file.unlink(missing_ok=True)


if __name__ == '__main__':
    sys.exit(main())
