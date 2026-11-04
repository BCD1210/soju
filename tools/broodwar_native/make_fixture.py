"""Build a private, deterministic UMS test map using terrain from an owned map.

Only writes the requested output map. No Blizzard map or asset is distributed.
Requires a locally built StormLib shared library.
"""
import argparse
import ctypes as C
import pathlib
import struct
import tempfile

def storm_library(path):
    lib=C.CDLL(str(path))
    signatures={
      "SFileOpenArchive":([C.c_char_p,C.c_uint32,C.c_uint32,C.POINTER(C.c_void_p)],C.c_bool),
      "SFileOpenFileEx":([C.c_void_p,C.c_char_p,C.c_uint32,C.POINTER(C.c_void_p)],C.c_bool),
      "SFileGetFileSize":([C.c_void_p,C.POINTER(C.c_uint32)],C.c_uint32),
      "SFileReadFile":([C.c_void_p,C.c_void_p,C.c_uint32,C.POINTER(C.c_uint32),C.c_void_p],C.c_bool),
      "SFileCloseFile":([C.c_void_p],C.c_bool),
      "SFileCloseArchive":([C.c_void_p],C.c_bool),
      "SFileCreateArchive":([C.c_char_p,C.c_uint32,C.c_uint32,C.POINTER(C.c_void_p)],C.c_bool),
      "SFileAddFileEx":([C.c_void_p,C.c_char_p,C.c_char_p,C.c_uint32,C.c_uint32,C.c_uint32],C.c_bool)}
    for name,(args,result) in signatures.items():
        f=getattr(lib,name);f.argtypes=args;f.restype=result
    return lib

def read_chk(lib,path,member=b"staredit\\scenario.chk"):
    archive=C.c_void_p();file=C.c_void_p()
    if not lib.SFileOpenArchive(bytes(path),0,0x100,C.byref(archive)):raise RuntimeError("cannot open map")
    try:
        if not lib.SFileOpenFileEx(archive,member,0,C.byref(file)):raise RuntimeError("missing CHK")
        try:
            size=lib.SFileGetFileSize(file,None)
            if size>16_000_000:raise RuntimeError("CHK too large")
            data=C.create_string_buffer(size);read=C.c_uint32()
            if not lib.SFileReadFile(file,data,size,C.byref(read),None) or read.value!=size:raise RuntimeError("short read")
            return data.raw
        finally:lib.SFileCloseFile(file)
    finally:lib.SFileCloseArchive(archive)

def sections(data):
    pos=0;result=[]
    while pos<len(data):
        if pos+8>len(data):raise ValueError("truncated CHK header")
        name,size=struct.unpack_from("<4sI",data,pos);pos+=8
        if pos+size>len(data):raise ValueError("truncated CHK section")
        result.append((name,data[pos:pos+size]));pos+=size
    return result

def unit(serial,x,y,kind,owner=0,resources=0):
    return struct.pack("<I6H4BIHHII",serial,x,y,kind,0,0,0,owner,100,100,100,resources,0,0,0,0)

def build(lib,source,destination,combat=False):
    original=sections(read_chk(lib,source))
    data=dict(original)
    units=[data[b"UNIT"][i:i+36] for i in range(0,len(data[b"UNIT"]),36)]
    player=next(i for i,owner in enumerate(data[b"OWNR"][:8]) if owner==6)
    starts=[struct.unpack_from("<HH",u,4) for u in units if struct.unpack_from("<H",u,8)[0]==214 and u[16]==player]
    if not starts:raise RuntimeError("source has no starting positions")
    x,y=starts[0]
    # Original neutral resources and terrain are retained. No AI or victory triggers.
    neutral=[u for u in units if struct.unpack_from("<H",u,8)[0] in (176,177,178,188,214)]
    placed=[unit(10000,x,y,106,player),unit(10001,x-75,y+70,7,player),unit(10002,x-45,y+70,7,player),
            unit(10003,x+15,y+70,0,player),unit(10004,x+45,y+70,0,player)]
    owners=bytearray(12);owners[player]=6
    races=bytearray(data[b"SIDE"]);races[player]=1
    forces=bytearray(20)
    for slot in range(8):forces[slot]=0 if slot==player else 1
    replacements={b"UNIT":b"".join(neutral+placed),b"TRIG":bytes(2400),b"MBRF":bytes(2400),
                  b"OWNR":bytes(owners),b"IOWN":bytes(owners),b"SIDE":bytes(races),b"FORC":bytes(forces)}
    if combat:
        enemy=1 if player!=1 else 0
        placed.extend([unit(10005,x-420,y+70,0,enemy),unit(10006,x-470,y+120,7,enemy)])
        owners[enemy]=5
        replacements.update({b"UNIT":b"".join(neutral+placed),b"OWNR":bytes(owners),b"IOWN":bytes(owners)})
    packed=b"".join(name+struct.pack("<I",len(replacements.get(name,value)))+replacements.get(name,value)
                    for name,value in original)
    destination.parent.mkdir(parents=True,exist_ok=True)
    if destination.exists():raise RuntimeError("output already exists; choose a new path")
    with tempfile.TemporaryDirectory() as temp:
        chk=pathlib.Path(temp)/"scenario.chk";chk.write_bytes(packed)
        archive=C.c_void_p()
        if not lib.SFileCreateArchive(bytes(destination),0,16,C.byref(archive)):raise RuntimeError("cannot create MPQ")
        try:
            if not lib.SFileAddFileEx(archive,bytes(chk),b"staredit\\scenario.chk",0x200,2,2):raise RuntimeError("cannot write CHK")
        finally:lib.SFileCloseArchive(archive)
    written=dict(sections(read_chk(lib,destination)))
    assert written[b"UNIT"]==replacements[b"UNIT"]
    for key in (b"DIM ",b"ERA ",b"MTXM",b"THG2",b"DD2 "):
        assert written.get(key)==data.get(key), "Terrain and doodads must remain unchanged"
    return {"map":str(destination),"start":[x,y],"units":len(neutral)+len(placed)}

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--storm",type=pathlib.Path,required=True)
    p.add_argument("--source",type=pathlib.Path,required=True)
    p.add_argument("--output",type=pathlib.Path,required=True)
    p.add_argument("--combat",action="store_true")
    a=p.parse_args()
    print(build(storm_library(a.storm),a.source,a.output,a.combat))
