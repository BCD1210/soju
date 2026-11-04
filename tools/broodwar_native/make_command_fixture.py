"""Create a fresh private per-race command fixture from an owned scenario.

Research/upgrade durations are 24 native frames; costs remain loaded DAT costs.
Unit production uses original durations. No scenario or image data is distributed.
"""
import argparse
import ctypes as C
import hashlib
import json
from pathlib import Path
import struct
import tempfile

from client import RUNTIME, request
from make_acceptance_fixture import build as base_fixture
from make_fixture import read_chk, sections, storm_library, unit
from map_assets import dependency

BUILDINGS = {
    'terran': [106,109,111,112,113,114,116,122,123,125,107,108,115,117,118,120,106,116],
    'zerg': [131,132,133,135,136,138,139,140,141,142,143,143],
    'protoss': [154,155,156,159,160,163,164,165,166,167,169,170,171,172],
}
UNITS = {
    'terran': [7,0,0,1,2,5,8,9,11,12,34,9,9,34,34],
    'zerg': [41,37,38,38,43,43,45,46,42,45,45,46,46],
    'protoss': [64,65,66,67,67,63,69,71,60,63,63,71],
}


def build(source: Path, output: Path, race: str, researched: bool) -> None:
    if output.exists():
        raise ValueError('Use a fresh output path')
    metadata = request('unit_types')
    lib = storm_library(dependency(RUNTIME, 'StormLib'))
    with tempfile.TemporaryDirectory() as temporary:
        template = Path(temporary)/'template.scx'
        base_fixture(source, template)
        original = sections(read_chk(lib, template))
        data = dict(original)
        width, height = struct.unpack('<HH', data[b'DIM '])
        layout = [(214,16,16)]
        layout += [(kind,16+(index%5)*9,16+(index//5)*10) for index,kind in enumerate(BUILDINGS[race])]
        layout += [(kind,18+(index%8)*4,49+(index//8)*4) for index,kind in enumerate(UNITS[race])]
        if race == 'protoss':
            layout += [(156,12+x*9,20+y*10) for y in range(3) for x in range(6)]
        if race == 'zerg':
            layout += [(131,16+x*9,42) for x in range(5)]
        layout += [(109 if race=='terran' else 42 if race=='zerg' else 156,76+(i%4)*4,16+(i//4)*4) for i in range(16)]
        records = [unit(40000+i,x*32,y*32,kind) for i,(kind,x,y) in enumerate(layout)]
        # Targets are isolated from the production area and owned passengers.
        records += [unit(41000+i,(22+i*6)*32,64*32,kind,1) for i,kind in enumerate([5,0,0,65,67])]
        for i,record in enumerate(records):
            record=bytearray(record);struct.pack_into('<H',record,14,8);records[i]=bytes(record)
        replacements = {b'UNIT':b''.join(records),b'SIDE':bytes([{'zerg':0,'terran':1,'protoss':2}[race]])+bytes(11)}
        tech = bytearray(data[b'PTEx'])
        for i in range(44):
            tech[44*12+i] = int(researched)
        # Innate abilities remain innate in research fixtures.
        for i in [4,6,12,14,18,23,26,28,29,33,34]:
            tech[44*12+i] = 1
        replacements[b'PTEx'] = bytes(tech)
        tech_stats = bytearray(396)
        for row in metadata['technologies']:
            i=row['id']
            for offset,value in [(44,row['mineralCost']),(132,row['gasCost']),(220,24),(308,row['energy'])]:
                struct.pack_into('<H',tech_stats,offset+i*2,value)
        replacements[b'TECx'] = bytes(tech_stats)
        upgrade_stats = bytearray(794)
        for row in metadata['upgrades']:
            i=row['id']
            for offset,value in [(62,row['mineralCost']),(184,row['mineralFactor']),(306,row['gasCost']),(428,row['gasFactor']),(550,24),(672,0)]:
                struct.pack_into('<H',upgrade_stats,offset+i*2,value)
        replacements[b'UPGx'] = bytes(upgrade_stats)
        max_levels=[3 if i<16 else 1 if i in set(range(16,45))-{18} or i in {47,49,51,52,53,54} else 0 for i in range(61)]
        limits=bytearray(max_levels*12+[0]*(61*12)+max_levels+[0]*61+[0]*(61*12))
        if researched:
            limits[61*12+24]=1  # Ventral Sacs transport fixture.
        replacements[b'PUPx']=bytes(limits)
        for name in (b'TECx',b'UPGx',b'PTEx',b'PUPx'):
            if len(replacements[name])!=len(data[name]):
                raise ValueError(f'Invalid scenario section size: {name!r}')
        packed=b''.join(name+struct.pack('<I',len(replacements.get(name,value)))+replacements.get(name,value) for name,value in original)
        present={name for name,_ in original}
        packed+=b''.join(name+struct.pack('<I',len(value))+value for name,value in replacements.items() if name not in present)
        chk=Path(temporary)/'scenario.chk';chk.write_bytes(packed)
        output.parent.mkdir(parents=True,exist_ok=True)
        archive=C.c_void_p()
        if not lib.SFileCreateArchive(bytes(output),0,16,C.byref(archive)):
            raise RuntimeError('Cannot create private archive')
        try:
            if not lib.SFileAddFileEx(archive,bytes(chk),b'staredit\\scenario.chk',0x200,2,2):
                raise RuntimeError('Cannot write private scenario')
        finally:
            lib.SFileCloseArchive(archive)
    output.with_suffix('.provenance.json').write_text(json.dumps({
        'race':race,'researched':researched,'source':str(source),'sourceSha256':hashlib.sha256(source.read_bytes()).hexdigest(),
        'sha256':hashlib.sha256(output.read_bytes()).hexdigest(),'researchFrames':24,'upgradeFrames':24,
        'unitDurations':'original','costs':'native DAT','width':width,'height':height},indent=2)+'\n')


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source',type=Path);parser.add_argument('output',type=Path)
    parser.add_argument('--race',choices=list(BUILDINGS),required=True)
    parser.add_argument('--researched',action='store_true')
    args=parser.parse_args();build(args.source,args.output,args.race,args.researched)
