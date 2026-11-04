# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
# Run: python3 tools/broodwar_native/make_tileset_fixtures.py SOURCE.scx
"""Create eight private terrain/doodad acceptance maps from owned CV5 tables."""
import ctypes as C
import json
from pathlib import Path
import struct
import sys
import tempfile

from client import RUNTIME
from make_fixture import read_chk, sections, storm_library
from map_assets import dependency
from scene_assets import Casc


def prepare(source:Path) -> None:
    """Use native CV5 doodad overlay IDs, flags and tile footprints."""
    lib=storm_library(dependency(RUNTIME,'StormLib'))
    original=sections(read_chk(lib,source));data=dict(original)
    width,height=struct.unpack('<HH',data[b'DIM '])
    casc=Casc(dependency(RUNTIME,'CascLib'),RUNTIME/'bottle/drive_c/Program Files (x86)/StarCraft')
    reports=[]
    try:
        for era,name in enumerate(('badlands','platform','install','ashworld','jungle','desert','ice','twilight')):
            cv5=casc.read(f'tileset/{name}.cv5');vf4=casc.read(f'tileset/{name}.vf4')
            records=list(struct.iter_unpack('<26H',cv5))
            walkable=[i for i,row in enumerate(records) if row[0]>=2 and
                      all(f&1 for f in struct.unpack_from('<16H',vf4,row[10]*32))]
            base=next((i for i in walkable if records[i][1]&0x80==0),walkable[0])
            tiles=[base*16]*(width*height);sprites=[];selected=[];seen=set()
            for group,row in enumerate(records):
                if row[0]!=1 or row[1]&0x1000==0 or row[2]>=517 or row[6] in seen or not 0<row[7]<=5 or not 0<row[8]<=5:
                    continue
                seen.add(row[6]);x=20+len(selected)*7;y=19
                for dy in range(row[8]):
                    for dx in range(row[7]):
                        if records[group+dy][10+dx]:tiles[(y+dy)*width+x+dx]=(group+dy)*16+dx
                sx=x*32+row[7]*16;sy=y*32+row[8]*16
                sprites.append(struct.pack('<3H2BH',row[2],sx,sy,11,0,row[1]))
                selected.append({'sprite':row[2],'group':group,'doodad':row[6],'x':sx,'y':sy})
                if len(selected)==3:break
            assert selected, f'No supported sprite doodad in {name}'
            replacements={b'ERA ':struct.pack('<H',era),b'MTXM':struct.pack(f'<{len(tiles)}H',*tiles),b'THG2':b''.join(sprites),b'DD2 ':b''}
            packed=b''.join(key+struct.pack('<I',len(replacements.get(key,value)))+replacements.get(key,value) for key,value in original)
            destination=RUNTIME/f'Soju-Acceptance-M4-{name}.scx'
            if destination.exists():
                assert read_chk(lib,destination)==packed, f'Different fixture exists: {destination}'
                reports.append({'tileset':name,'era':era,'map':str(destination),'baseGroup':base,'doodads':selected})
                continue
            with tempfile.TemporaryDirectory() as temp:
                chk=Path(temp)/'scenario.chk';chk.write_bytes(packed);archive=C.c_void_p()
                assert lib.SFileCreateArchive(bytes(destination),0,16,C.byref(archive)), 'Cannot create fixture'
                try:
                    assert lib.SFileAddFileEx(archive,bytes(chk),b'staredit\\scenario.chk',0x200,2,2), 'Cannot write fixture'
                finally:lib.SFileCloseArchive(archive)
            reports.append({'tileset':name,'era':era,'map':str(destination),'baseGroup':base,'doodads':selected})
        (RUNTIME/'3d-proof/m4/tileset-fixtures.json').write_text(json.dumps(reports,indent=2)+'\n')
        print(json.dumps(reports,indent=2))
    finally:casc.close()


if __name__=='__main__':
    prepare(Path(sys.argv[1]).expanduser())
