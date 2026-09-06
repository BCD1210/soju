"""Extract a game's existing Windows icon locally; never downloads artwork."""
import hashlib
from pathlib import Path
import struct


def executable_icon(executable, cache):
    try:
        stat = executable.stat()
        key = hashlib.sha256((str(executable) + str(stat.st_mtime_ns) + str(stat.st_size)).encode()).hexdigest()
        output = Path(cache) / (key + '.ico')
        if output.is_file(): return str(output)
        with executable.open('rb') as stream:
            def read(at, size):
                if at < 0 or size < 0 or size > 8 * 1024 * 1024 or at + size > stat.st_size:
                    raise ValueError('invalid PE offset')
                stream.seek(at); data = stream.read(size)
                if len(data) != size: raise ValueError('truncated executable')
                return data
            def number(at, form='<I'):
                return struct.unpack(form, read(at, struct.calcsize(form)))[0]
            if read(0, 2) != b'MZ': return None
            pe = number(0x3c)
            if read(pe, 4) != b'PE\x00\x00': return None
            sections, optional_size = number(pe + 6, '<H'), number(pe + 20, '<H')
            if not 1 <= sections <= 96: return None
            optional = pe + 24
            magic = number(optional, '<H')
            directory = 112 if magic == 0x20b else 96 if magic == 0x10b else None
            if directory is None or optional_size < directory + 24: return None
            resource_rva = number(optional + directory + 16)
            segments = []
            for i in range(sections):
                section = optional + optional_size + i * 40
                virtual_size, address, raw_size, raw = struct.unpack('<IIII', read(section + 8, 16))
                segments.append((address, raw_size, raw))
            def offset(rva):
                for address, size, raw in segments:
                    if address <= rva < address + size: return raw + rva - address
                raise ValueError('resource outside file sections')
            base = offset(resource_rva)
            def entries(relative):
                named, ids = struct.unpack('<HH', read(base + relative + 12, 4))
                if named + ids > 4096: raise ValueError('too many resources')
                return [struct.unpack('<II', read(base + relative + 16 + i * 8, 8)) for i in range(named + ids)]
            types = dict(entries(0))
            def resource(kind, identity=None):
                node = types[kind]
                if not node & 0x80000000: raise ValueError('expected resource directory')
                choices = dict(entries(node & 0x7fffffff))
                node = choices[identity] if identity is not None else next(iter(choices.values()))
                for _ in range(4):
                    if not node & 0x80000000:
                        rva, size = struct.unpack('<II', read(base + node, 8))
                        return read(offset(rva), size)
                    children = entries(node & 0x7fffffff)
                    node = children[0][1]
                raise ValueError('resource nesting too deep')
            group = resource(14)
            reserved, kind, count = struct.unpack_from('<HHH', group)
            if reserved != 0 or kind != 1 or not 1 <= count <= 128 or len(group) < 6 + count * 14:
                return None
            header = bytearray(struct.pack('<HHH', 0, 1, count))
            payload = bytearray(); start = 6 + count * 16
            for i in range(count):
                entry = group[6 + i * 14:20 + i * 14]
                identity = struct.unpack_from('<H', entry, 12)[0]
                icon = resource(3, identity)
                header.extend(entry[:8] + struct.pack('<II', len(icon), start + len(payload)))
                payload.extend(icon)
                if len(payload) > 8 * 1024 * 1024: raise ValueError('icon too large')
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(header + payload)
        return str(output)
    except (OSError, ValueError, KeyError, IndexError, StopIteration, struct.error):
        return None
