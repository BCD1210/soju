# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
# Run: python3 tools/broodwar_native/make_acceptance_fixture.py SOURCE.scx OUTPUT.scx
"""Build a private command acceptance map from an owned Brood War map."""
import ctypes as C
import hashlib
import json
from pathlib import Path
import struct
import sys
import tempfile

from client import RUNTIME
from make_fixture import read_chk, sections, storm_library, unit
from map_assets import dependency


def build(source: Path, destination: Path) -> None:
    """Retain owned terrain/metadata and replace only the fixture scenario."""
    lib = storm_library(dependency(RUNTIME, 'StormLib'))
    original = sections(read_chk(lib, source))
    data = dict(original)
    assert struct.unpack('<H', data[b'VER '])[0] in (205, 206), 'Brood War source required'
    width, height = struct.unpack('<HH', data[b'DIM '])
    assert width >= 64 and height >= 64, 'Fixture needs at least 64 by 64 tiles'
    assert not destination.exists(), 'Choose an unused output path'
    player = 0
    placed = []
    # Terran production, transport, mode users and Zerg production share the human slot.
    layout = [(214,24,16),(106,24,16),(111,32,16),(112,40,16),(113,48,16),
              (114,56,16),(116,24,23),(122,32,23),(123,40,23),(109,48,23),
              (7,24,28),(7,26,28),(0,28,28),(0,30,28),(5,32,28),(8,36,28),
              (11,40,28),(125,44,28),(131,24,40),(142,32,40),(135,40,40),
              (41,25,44),(41,27,44),(37,30,44),(38,32,44),(42,36,44),
              (154,48,40),(160,56,40),(64,48,44),(65,50,44),
              (176,20,16),(176,20,18),(188,20,23)]
    for index, (kind, x, y) in enumerate(layout):
        placed.append(unit(30000+index,x*32,y*32,kind,11 if kind in (176,188) else player,5000 if kind in (176,188) else 0))
    for index in range(8):
        placed.append(unit(31000+index,(55+index%4)*32,(30+index//4*2)*32,37,1))
    owners = bytes([6,5,0,0,0,0,0,0,0,0,0,0])
    # One Always trigger grants resources once. No victory or autonomous build scripts.
    trigger = bytearray(2400)
    trigger[15] = 22
    struct.pack_into('<6IH4B2x',trigger,320,0,0,0,0,player,50000,0,26,7,4,0)
    struct.pack_into('<6IH4B2x',trigger,352,0,0,0,0,player,50000,1,26,7,4,0)
    trigger[2372+player] = 1
    tech = bytearray([1]*(44*12) + [0]*(44*12) + [1]*44 + [0]*44 + [0]*(44*12))
    for kind in (0,5,9,10,11):
        tech[44*12+player*44+kind] = 1
    replacements = {b'UNIT':b''.join(placed),b'OWNR':owners,b'IOWN':owners,
                    b'SIDE':bytes([1,0,0,0,0,0,0,0,0,0,0,0]),
                    b'FORC':bytes([0,1,2,2,2,2,2,2])+bytes(12),
                    b'TRIG':bytes(trigger),b'MBRF':bytes(2400),b'PTEx':bytes(tech),
                    b'MASK':bytes(width*height),b'THG2':b'',b'DD2 ':b''}
    # Use the known walkable start tile from the owned source across the test arena.
    records = [data[b'UNIT'][i:i+36] for i in range(0,len(data[b'UNIT']),36)]
    start = next(u for u in records if struct.unpack_from('<H',u,8)[0] == 214)
    x,y = struct.unpack_from('<HH',start,4)
    tile = struct.unpack_from('<H',data[b'MTXM'],((y//32)*width+x//32)*2)[0]
    replacements[b'MTXM'] = struct.pack('<H',tile)*(width*height)
    # Enable all production; native handlers still enforce prerequisites and costs.
    replacements[b'PUNI'] = bytes([1]*5700)
    packed = b''.join(name+struct.pack('<I',len(replacements.get(name,value)))+replacements.get(name,value) for name,value in original)
    destination.parent.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory() as temp:
        chk = Path(temp)/'scenario.chk'
        chk.write_bytes(packed)
        archive = C.c_void_p()
        assert lib.SFileCreateArchive(bytes(destination),0,16,C.byref(archive)), 'Cannot create fixture archive'
        try:
            assert lib.SFileAddFileEx(archive,bytes(chk),b'staredit\\scenario.chk',0x200,2,2), 'Cannot write fixture'
        finally:
            lib.SFileCloseArchive(archive)
    report = {'map':str(destination),'source':str(source),'sourceSha256':hashlib.sha256(source.read_bytes()).hexdigest(),
              'sha256':hashlib.sha256(destination.read_bytes()).hexdigest(),'human':player,'units':len(placed),
              'terrainTile':tile,'purpose':'Private extended-command and presentation acceptance'}
    destination.with_suffix('.provenance.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))


if __name__ == '__main__':
    build(Path(sys.argv[1]).expanduser(),Path(sys.argv[2]).expanduser())
