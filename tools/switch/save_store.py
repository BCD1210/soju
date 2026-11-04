"""Bounded, atomic, exclusively locked per-NRO save snapshots."""
import base64
import fcntl
import json
import os
from pathlib import Path
import secrets
import stat

from memory_files import MemoryFiles, MAX_FILE_SIZE, MAX_TOTAL_SIZE, MAX_FILES

MAX_SNAPSHOT = 48 * 1024 * 1024
MAX_SAVES = 16

def valid_path(path):
    if not isinstance(path, str) or not path.startswith('/') or len(path.encode('utf-8')) >= 0x301:
        return False
    return path == '/' or (not path.endswith('/') and all(part not in ('', '.', '..') for part in path.split('/')[1:]))

def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError('duplicate JSON key')
        result[key] = value
    return result

class SaveStore:
    def __init__(self, home, game_hash):
        if len(game_hash) != 64 or any(c not in '0123456789abcdef' for c in game_hash):
            raise ValueError('invalid game identity')
        self.home = Path(home)
        self.game_hash = game_hash
        self.directory = self.home / 'saves' / game_hash
        self.path = self.directory / 'state.json'
        self.lock_fd = None
        self.dir_fd = None
        self.loaded = False

    def acquire(self):
        if self.lock_fd is not None:
            return
        self.home.mkdir(mode=0o700, parents=True, exist_ok=True)
        flags = os.O_RDONLY | os.O_DIRECTORY | getattr(os, 'O_NOFOLLOW', 0)
        home_fd = os.open(self.home, flags)
        saves_fd = None
        game_fd = None
        lock_fd = None
        try:
            try:
                os.mkdir('saves', 0o700, dir_fd=home_fd)
            except FileExistsError:
                pass
            saves_fd = os.open('saves', flags, dir_fd=home_fd)
            try:
                os.mkdir(self.game_hash, 0o700, dir_fd=saves_fd)
            except FileExistsError:
                pass
            game_fd = os.open(self.game_hash, flags, dir_fd=saves_fd)
            lock_fd = os.open('.lock', os.O_CREAT | os.O_RDWR | getattr(os, 'O_NOFOLLOW', 0), 0o600, dir_fd=game_fd)
            if not stat.S_ISREG(os.fstat(lock_fd).st_mode):
                raise ValueError('save lock must be a regular file')
            fcntl.flock(lock_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            self.dir_fd, self.lock_fd = game_fd, lock_fd
            game_fd = lock_fd = None
        finally:
            for fd in (lock_fd, game_fd, saves_fd, home_fd):
                if fd is not None:
                    os.close(fd)

    def close(self):
        if self.lock_fd is not None:
            os.close(self.lock_fd)
            self.lock_fd = None
        if self.dir_fd is not None:
            os.close(self.dir_fd)
            self.dir_fd = None
        self.loaded = False

    def load(self):
        self.acquire()
        flags = os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0)
        try:
            fd = os.open('state.json', flags, dir_fd=self.dir_fd)
        except FileNotFoundError:
            self.loaded = True
            return {}
        except OSError as exc:
            raise ValueError('cannot open save snapshot safely') from exc
        try:
            info = os.fstat(fd)
            if not stat.S_ISREG(info.st_mode) or info.st_size > MAX_SNAPSHOT:
                raise ValueError('invalid save snapshot')
            with os.fdopen(fd, 'rb') as source:
                fd = -1
                raw = source.read(MAX_SNAPSHOT + 1)
            if len(raw) > MAX_SNAPSHOT:
                raise ValueError('save snapshot too large')
        finally:
            if fd != -1:
                os.close(fd)
        try:
            payload = json.loads(raw, object_pairs_hook=unique_object)
            if not isinstance(payload, dict) or payload.get('version') != 1:
                raise ValueError('unsupported save snapshot')
            records = payload['saves']
            if not isinstance(records, list) or len(records) > MAX_SAVES:
                raise ValueError('invalid save count')
            result, ids, total = {}, set(), 0
            for record in records:
                attr = bytes.fromhex(record['attribute'])
                dirs = record['dirs']
                items = record['files']
                if (len(attr) != 64 or attr in result or attr[0x20] != 1 or
                        attr[8:24] != bytes([1]) + bytes(15) or any(attr[0x24:]) or
                        not isinstance(dirs, list) or len(dirs) > 256 or '/' not in dirs):
                    raise ValueError('invalid save metadata')
                if not all(valid_path(p) for p in dirs) or len(set(dirs)) != len(dirs):
                    raise ValueError('invalid save directories')
                if any(p != '/' and (p.rpartition('/')[0] or '/') not in dirs for p in dirs):
                    raise ValueError('save directory parent is missing')
                if not isinstance(items, dict) or len(items) > MAX_FILES:
                    raise ValueError('invalid save files')
                fs = MemoryFiles()
                for path, encoded in items.items():
                    if not valid_path(path) or path == '/' or path in dirs or (path.rpartition('/')[0] or '/') not in dirs:
                        raise ValueError('invalid save path')
                    if not isinstance(encoded, str) or len(encoded) > 4 * ((MAX_FILE_SIZE + 2) // 3):
                        raise ValueError('save file too large')
                    data = base64.b64decode(encoded, validate=True)
                    total += len(data)
                    if len(data) > MAX_FILE_SIZE or total > MAX_TOTAL_SIZE:
                        raise ValueError('save data exceeds limit')
                    fs.create(path, len(data))
                    fs.files[path][:] = data
                space, size, save_id = record['space'], record['size'], record['id']
                if not (type(space) is int and space == 1 and type(size) is int
                        and 0 <= size <= 128 * 1024 * 1024 and type(save_id) is int
                        and 0 < save_id < (1 << 64) and save_id not in ids
                        and sum(map(len, fs.files.values())) <= size):
                    raise ValueError('invalid save values')
                ids.add(save_id)
                result[attr] = {'dirs': set(dirs), 'files': fs, 'space': space, 'size': size, 'id': save_id}
            self.loaded = True
            return result
        except (KeyError, TypeError, ValueError, UnicodeDecodeError, json.JSONDecodeError) as exc:
            raise ValueError(f'invalid save snapshot: {exc}') from exc

    def save(self, saves):
        if not self.loaded or self.lock_fd is None:
            raise ValueError('save snapshot must be loaded and locked first')
        if len(saves) > MAX_SAVES:
            raise ValueError('too many save records')
        records, total = [], 0
        for attr, item in sorted(saves.items()):
            size = sum(map(len, item['files'].files.values()))
            if size > item['size']:
                raise ValueError('save data exceeds allocated size')
            total += size
            if total > MAX_TOTAL_SIZE:
                raise ValueError('save data exceeds limit')
            records.append({'attribute': attr.hex(), 'dirs': sorted(item['dirs']),
                            'files': {p: base64.b64encode(data).decode('ascii')
                                      for p, data in sorted(item['files'].files.items())},
                            'space': item['space'], 'size': item['size'], 'id': item['id']})
        raw = json.dumps({'version': 1, 'saves': records}, separators=(',', ':')).encode('utf-8')
        if len(raw) > MAX_SNAPSHOT:
            raise ValueError('save snapshot too large')
        try:
            old = os.stat('state.json', dir_fd=self.dir_fd, follow_symlinks=False)
            if not stat.S_ISREG(old.st_mode):
                raise ValueError('save file must be regular')
        except FileNotFoundError:
            pass
        name = '.soju-save-' + secrets.token_hex(16)
        fd = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0),
                     0o600, dir_fd=self.dir_fd)
        pending = True
        try:
            with os.fdopen(fd, 'wb') as out:
                out.write(raw)
                out.flush()
                os.fsync(out.fileno())
            os.replace(name, 'state.json', src_dir_fd=self.dir_fd, dst_dir_fd=self.dir_fd)
            pending = False
            os.fsync(self.dir_fd)
        finally:
            if pending:
                os.unlink(name, dir_fd=self.dir_fd)
