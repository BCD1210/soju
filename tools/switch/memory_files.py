"""Bounded, non-persistent guest file storage."""
MAX_FILES = 128
MAX_FILE_SIZE = 8 * 1024 * 1024
MAX_TOTAL_SIZE = 32 * 1024 * 1024

class MemoryFiles:
    def __init__(self):
        self.files = {}

    def resize(self, path, size):
        if size < 0 or size > MAX_FILE_SIZE:
            raise ValueError('virtual file exceeds size limit')
        old = self.files[path]
        if sum(map(len, self.files.values())) - len(old) + size > MAX_TOTAL_SIZE:
            raise ValueError('virtual filesystem exceeds size limit')
        if size > len(old):
            old.extend(bytes(size - len(old)))
        else:
            del old[size:]

    def create(self, path, size):
        if path in self.files or len(self.files) >= MAX_FILES:
            raise ValueError('file exists or virtual file limit reached')
        self.files[path] = bytearray()
        try:
            self.resize(path, size)
        except ValueError:
            del self.files[path]
            raise

    def read(self, path, offset, size):
        if offset < 0 or size < 0 or size > MAX_FILE_SIZE:
            raise ValueError('invalid virtual file read')
        return bytes(self.files[path][offset:offset + size])

    def write(self, path, offset, data, append=False):
        if offset < 0 or len(data) > MAX_FILE_SIZE:
            raise ValueError('invalid virtual file write')
        end = offset + len(data)
        if end > len(self.files[path]):
            if not append:
                raise ValueError('write extends file without append mode')
            self.resize(path, end)
        self.files[path][offset:end] = data
