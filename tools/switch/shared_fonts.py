"""Build a bounded virtual shared-font image from local host fallbacks."""

SHARED_FONT_SIZE = 0x1100000
MAX_FONT_SIZE = 15 * 1024 * 1024


def build_font_image(standard, unicode_bmp=b''):
    if not standard or len(standard) > 8 * 1024 * 1024:
        raise ValueError('invalid standard host font')
    if len(unicode_bmp) > MAX_FONT_SIZE:
        raise ValueError('Unicode host font exceeds limit')
    data = bytearray(standard)
    records = [(0, len(standard))]
    if unicode_bmp:
        offset = (len(data) + 0xfff) & ~0xfff
        if offset + len(unicode_bmp) > SHARED_FONT_SIZE:
            raise ValueError('host fonts exceed shared-memory limit')
        data.extend(bytes(offset - len(data)))
        data.extend(unicode_bmp)
        records.extend([(offset, len(unicode_bmp))] * 5)
    else:
        # Explicit Latin-only fallback; it does not supply missing CJK glyphs.
        records.extend([(0, len(standard))] * 5)
    return bytes(data), tuple(records)
