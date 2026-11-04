"""Create a private all-model UMS gallery from an owned map; no assets in git."""
import argparse
import ctypes as C
import hashlib
import json
import pathlib
import struct
import tempfile
from make_fixture import storm_library, read_chk, sections, unit
from client import RUNTIME


def gallery(lib,source,destination,catalog):
    original=sections(read_chk(lib,source));data=dict(original)
    if struct.unpack('<H',data[b'VER '])[0] not in (205,206):
        raise ValueError('Coverage gallery requires a Brood War format source map')
    width,height=struct.unpack('<HH',data[b'DIM '])
    player=next(i for i,v in enumerate(data[b'OWNR'][:8]) if v==6)
    units=[data[b'UNIT'][i:i+36] for i in range(0,len(data[b'UNIT']),36)]
    start=next(struct.unpack_from('<HH',u,4) for u in units if struct.unpack_from('<H',u,8)[0]==214 and u[16]==player)
    tile=struct.unpack_from('<H',data[b'MTXM'],((start[1]//32)*width+start[0]//32)*2)[0]
    entries=json.loads(catalog.read_text())['models'];placed=[unit(19999,6*32,6*32,214,player)];records=[]
    # All owned units share the human slot so the gallery cannot fight itself.
    columns=(width-12)//7
    for index,kind in enumerate(sorted(map(int,entries))):
        x=(6+(index%columns)*7)*32;y=(6+(index//columns)*7)*32
        if y>(height-6)*32:raise ValueError('Source map too small for gallery')
        owner=11 if kind in (176,177,178,188) else player
        placed.append(unit(20000+index,x,y,kind,owner,resources=5000 if owner==11 else 0))
        records.append({'type':kind,'name':entries[str(kind)]['name'],'x':x,'y':y,'owner':owner})
    owners=bytearray(data[b'OWNR'])
    for slot in range(8):
        if slot!=player and owners[slot]==6:owners[slot]=5
    owners[player]=6
    races=bytearray(data[b'SIDE']);races[player]=1
    replacements={b'UNIT':b''.join(placed),b'TRIG':bytes(2400),b'MBRF':bytes(2400),
        b'OWNR':bytes(owners),b'IOWN':bytes(owners),b'SIDE':bytes(races),b'MTXM':struct.pack('<H',tile)*(width*height),
        b'MASK':bytes(width*height)}
    packed=b''.join(name+struct.pack('<I',len(replacements.get(name,value)))+replacements.get(name,value) for name,value in original)
    if destination.exists():raise ValueError('Output already exists; choose a new map')
    destination.parent.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        chk=pathlib.Path(tmp)/'scenario.chk';chk.write_bytes(packed);archive=C.c_void_p()
        if not lib.SFileCreateArchive(bytes(destination),0,16,C.byref(archive)):raise RuntimeError('Cannot create MPQ')
        try:
            if not lib.SFileAddFileEx(archive,bytes(chk),b'staredit\\scenario.chk',0x200,2,2):raise RuntimeError('Cannot write CHK')
        finally:lib.SFileCloseArchive(archive)
    assert dict(sections(read_chk(lib,destination)))[b'UNIT']==replacements[b'UNIT']
    report={'map':str(destination),'source':str(source),'sourceSha256':hashlib.sha256(source.read_bytes()).hexdigest(),
        'sha256':hashlib.sha256(destination.read_bytes()).hexdigest(),'terrainTile':tile,'player':player,'units':records,
        'purpose':'Private offline model coverage gallery; owned map terrain repeated into a flat test field'}
    destination.with_suffix('.provenance.json').write_text(json.dumps(report,indent=2)+'\n')
    return report

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--storm',type=pathlib.Path,required=True)
    parser.add_argument('--source',type=pathlib.Path,required=True)
    parser.add_argument('--output',type=pathlib.Path,required=True)
    parser.add_argument('--catalog',type=pathlib.Path,required=True)
    a=parser.parse_args();report=gallery(storm_library(a.storm),a.source,a.output,a.catalog)
    print(json.dumps({'map':report['map'],'units':len(report['units'])},indent=2))
