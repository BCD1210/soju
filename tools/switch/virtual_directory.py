"""In-memory SD directory enumeration for the experimental runtime."""
import struct

ENTRY_SIZE = 0x310

class Directory:
    def __init__(self, path, mode, directories, file_size, files=None, include_self=True):
        if mode & ~0x80000003 or not mode & 3:
            raise ValueError("unsupported directory mode")
        entries = []
        if mode & 1:
            entries.extend((p.rsplit('/', 1)[-1], 0, 0) for p in directories
                           if p != '/' and (p.rpartition('/')[0] or '/') == path)
        if mode & 2:
            if include_self and path == '/switch':
                entries.append(('soju-current.nro', 1, 0 if mode & 0x80000000 else file_size))
            entries.extend((p.rsplit('/', 1)[-1], 1, 0 if mode & 0x80000000 else len(data))
                           for p, data in (files or {}).items()
                           if (p.rpartition('/')[0] or '/') == path)
        self.entries = sorted(entries)
        self.position = 0

    def read(self, byte_capacity):
        if byte_capacity < 0 or byte_capacity > 8 * 1024 * 1024:
            raise ValueError("invalid directory buffer capacity")
        count = min(len(self.entries) - self.position, byte_capacity // ENTRY_SIZE)
        result = bytearray(count * ENTRY_SIZE)
        for index, (name, kind, size) in enumerate(self.entries[self.position:self.position + count]):
            encoded = name.encode('utf-8')
            if len(encoded) >= 0x301:
                raise ValueError("directory entry name is too long")
            offset = index * ENTRY_SIZE
            result[offset:offset + len(encoded)] = encoded
            result[offset + 0x304] = kind
            struct.pack_into('<q', result, offset + 0x308, size)
        self.position += count
        return bytes(result), count
