"""Export button labels from the owned installation to the private runtime."""
import json
import struct
from client import RUNTIME
from map_assets import dependency
from scene_assets import Casc


def decode(data: bytes) -> dict:
    count = struct.unpack_from('<H', data)[0]
    if 2 + count * 2 > len(data):
        raise ValueError('Truncated string index')
    strings = {}
    for index in range(1, count + 1):
        offset = struct.unpack_from('<H', data, index * 2)[0]
        if offset >= len(data):
            raise ValueError('String offset outside table')
        # Command labels have key and category bytes, then the colored label.
        if offset + 1 < len(data) and data[offset + 1] < 32:
            raw = data[offset + 2:].split(b'\0', 1)[0]
            label = ''.join(chr(c) for c in raw if c >= 32).strip()
            key = 'Esc' if data[offset] == 27 else chr(data[offset]).upper() if data[offset] >= 32 else ''
            strings[str(index)] = {'label': label, 'hotkey': key}
    return strings


def prepare() -> None:
    casc = Casc(dependency(RUNTIME, 'CascLib'), RUNTIME/'bottle/drive_c/Program Files (x86)/StarCraft')
    try:
        data = casc.read('rez/stat_txt.tbl')
    finally:
        casc.close()
    (RUNTIME/'button-strings.json').write_text(json.dumps(decode(data), ensure_ascii=False))


if __name__ == '__main__':
    prepare()
