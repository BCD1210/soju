#!/usr/bin/env python3
"""Prepare a local-only Unicode fallback from an installed macOS font."""
import os
from pathlib import Path
import sys
import tempfile

from fontTools import subset
from fontTools.ttLib import TTFont

MAX_FONT = 15 * 1024 * 1024
SOURCE = Path('/Library/Fonts/Arial Unicode.ttf')


def main(home):
    if not SOURCE.is_file():
        print('Optional Unicode host font not available; Latin fallback will be used.')
        return
    home.mkdir(parents=True, exist_ok=True)
    dest = home / 'host-unicode-bmp.ttf'
    if dest.is_file() and 0 < dest.stat().st_size <= MAX_FONT:
        return
    font = TTFont(SOURCE)
    options = subset.Options()
    options.hinting = False
    worker = subset.Subsetter(options=options)
    worker.populate(unicodes=range(0x10000))
    worker.subset(font)
    fd, temporary = tempfile.mkstemp(prefix='.soju-font-', suffix='.ttf', dir=home)
    os.close(fd)
    try:
        font.save(temporary)
        if not 0 < os.path.getsize(temporary) <= MAX_FONT:
            raise ValueError('generated Unicode fallback exceeds shared-memory limit')
        check = TTFont(temporary, lazy=True)
        try:
            cmap = check.getBestCmap()
            if not cmap or not all(char in cmap for char in (0x41, 0x4e2d, 0x56fd, 0xd55c)):
                raise ValueError('generated Unicode fallback lacks expected glyph coverage')
        finally:
            check.close()
        os.replace(temporary, dest)
    finally:
        font.close()
        if os.path.exists(temporary):
            os.unlink(temporary)
    print('Prepared a local Unicode fallback from the installed host font.')


if __name__ == '__main__':
    main(Path(sys.argv[1]))
