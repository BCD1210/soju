"""Prepare original local Remastered art for the native scene renderer.

CASC access uses CascLib's public API; ANIM/DDS-GRP metadata follows the formats
documented by neivv/animosity. No game assets are downloaded or distributed.
"""
import argparse
import ctypes as C
import json
import hashlib
import pathlib
import struct
from client import RUNTIME, request
from make_fixture import storm_library as storm, read_chk, sections

class Casc:
    def __init__(self, library, game):
        self.lib=C.CDLL(str(library))
        for name,args in {
            "CascOpenStorage":[C.c_char_p,C.c_uint32,C.POINTER(C.c_void_p)],
            "CascOpenFile":[C.c_void_p,C.c_char_p,C.c_uint32,C.c_uint32,C.POINTER(C.c_void_p)],
            "CascGetFileSize64":[C.c_void_p,C.POINTER(C.c_uint64)],
            "CascReadFile":[C.c_void_p,C.c_void_p,C.c_uint32,C.POINTER(C.c_uint32)],
            "CascCloseFile":[C.c_void_p],"CascCloseStorage":[C.c_void_p],
        }.items():
            fn=getattr(self.lib,name);fn.argtypes=args;fn.restype=C.c_bool
        self.handle=C.c_void_p()
        if not self.lib.CascOpenStorage(str(game).encode(),0,C.byref(self.handle)):
            raise RuntimeError("Could not open the installed game's CASC storage")
    def close(self):
        if self.handle:
            self.lib.CascCloseStorage(self.handle);self.handle=C.c_void_p()
    def read(self,name):
        handle=C.c_void_p()
        if not self.lib.CascOpenFile(self.handle,name.encode(),0,0,C.byref(handle)):
            raise FileNotFoundError(name)
        try:
            size=C.c_uint64()
            if not self.lib.CascGetFileSize64(handle,C.byref(size)) or not 0<size.value<=128*1024*1024:
                raise ValueError("Invalid or excessive asset size: "+name)
            data=C.create_string_buffer(size.value);got=C.c_uint32()
            if not self.lib.CascReadFile(handle,data,size.value,C.byref(got)) or got.value!=size.value:
                raise RuntimeError("Incomplete local asset: "+name)
            return data.raw
        finally:self.lib.CascCloseFile(handle)

def unpack(fmt,data,offset):
    n=struct.calcsize(fmt)
    if offset<0 or offset+n>len(data):raise ValueError("Truncated graphics metadata")
    return struct.unpack_from(fmt,data,offset)

def anim(data):
    if data[:4]!=b"ANIM":raise ValueError("Not an ANIM")
    scale,kind,_,layers,entries=unpack("<BBHHH",data,4)
    if kind!=2 or entries!=1 or scale not in(2,4) or not 1<=layers<=10:
        raise ValueError("Only individual HD ANIM files are supported")
    names=[data[12+i*32:44+i*32].split(b"\0",1)[0].decode("ascii") for i in range(layers)]
    count,ref,width,height,frame_offset=unpack("<4HI",data,0x14c)
    if ref!=0xffff:raise ValueError("Unexpected external ANIM reference")
    frames=[]
    for i in range(count):
        x,y,ox,oy,w,h,_=unpack("<HHhhHHI",data,frame_offset+16*i)
        frames.append({"x":x,"y":y,"ox":ox,"oy":oy,"w":w,"h":h})
    textures={}
    for i,name in enumerate(names):
        offset,size,w,h=unpack("<IIHH",data,0x158+i*12)
        if not offset:continue
        if offset+size>len(data) or size<4:raise ValueError("Texture outside ANIM")
        textures[name]=(data[offset:offset+size],w,h)
    return {"scale":scale,"width":width,"height":height,"frames":frames},textures

def ddsgrp(data):
    size,count,flags,version=unpack("<IHBB",data,0)
    if size!=len(data) or flags&0xf not in(2,4) or flags&0xf0 or version!=16:
        raise ValueError("Unsupported HD DDS-GRP")
    result=[];offset=8
    for _ in range(count):
        _,w,h,n=unpack("<IHHI",data,offset);offset+=12
        if offset+n>len(data):raise ValueError("DDS-GRP texture overrun")
        result.append((data[offset:offset+n],w,h));offset+=n
    return result

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--casc-lib",type=pathlib.Path,required=True)
    p.add_argument("--storm",type=pathlib.Path,required=True)
    p.add_argument("--map",type=pathlib.Path,required=True)
    p.add_argument("--game",type=pathlib.Path,default=RUNTIME/"bottle/drive_c/Program Files (x86)/StarCraft")
    p.add_argument("--out",type=pathlib.Path,default=RUNTIME/"scene-assets")
    p.add_argument("--terrain-only",action="store_true")
    a=p.parse_args()
    state=request();out=a.out;out.mkdir(parents=True,exist_ok=True)
    from map_assets import local_map
    if state["map"].get("path") and local_map(state["map"]["path"])!=a.map.resolve():
        raise ValueError("Selected map is not the map loaded in the original client")
    chunks=dict(sections(read_chk(storm(a.storm),a.map)))
    width,height=unpack("<HH",chunks[b"DIM "],0)
    tileset_id=unpack("<H",chunks[b"ERA "],0)[0]&7
    if tileset_id!=state["map"]["tileset"] or width*32!=state["map"]["width"] or height*32!=state["map"]["height"]:
        raise ValueError("Selected map does not match the live game dimensions/tileset")
    names=("badlands","platform","install","ashworld","jungle","desert","ice","twilight")
    tileset=names[tileset_id]
    c=Casc(a.casc_lib,a.game)
    manifest={"source":"local-owned-remastered-install","map":{"width":width,"height":height,"tileset":tileset},
              "images":{},"tiles":{},"missing":[]}
    try:
        cv5=c.read("tileset/"+tileset+".cv5")
        tile_graphics=ddsgrp(c.read("tileset/"+tileset+".dds.vr4"))
        mt=chunks[b"MTXM"]
        if len(mt)!=width*height*2:raise ValueError("Unexpected map tile count")
        indices=[]
        for (tile,) in struct.iter_unpack("<H",mt):
            megatile=unpack("<H",cv5,(tile>>4)*52+20+(tile&15)*2)[0]
            if megatile>=len(tile_graphics):raise ValueError("Map references absent tile")
            indices.append(megatile)
        manifest["map"]["tiles"]=indices
        manifest["map"]["tilesetId"]=tileset_id
        manifest["map"]["nativePath"]=state["map"].get("path", "")
        manifest["map"]["fileHash"]=hashlib.sha256(a.map.read_bytes()).hexdigest()
        manifest["map"]["terrainHash"]=hashlib.sha256(chunks[b"DIM "]+chunks[b"ERA "]+mt).hexdigest()
        from terrain3d import prepare
        terrain=prepare(width,height,indices,c.read("tileset/"+tileset+".vf4"))
        (out/"terrain3d.json").write_text(json.dumps(terrain,separators=(",",":")))
        manifest["map"]["terrain3d"]="terrain3d.json"
        for index in sorted(set(indices)):
            data,w,h=tile_graphics[index];name=f"tile-{index}.dds"
            (out/name).write_bytes(data)
            manifest["tiles"][str(index)]={"file":name,"width":w,"height":h}
        ids=set() if a.terrain_only else {u["image"]["id"] for u in state["units"] if u["image"]}
        relations=b"" if a.terrain_only else c.read("images.rel")
        for image_id in sorted(ids):
            source_id=image_id
            seen=set()
            while True:
                if source_id in seen:raise ValueError("Cyclic HD image relation")
                seen.add(source_id)
                mode,target=unpack("<II",relations,source_id*8)
                if mode!=0x200:break
                source_id=target
            found=None
            for prefix in ("","HD2/"):
                try:
                    found=c.read(f"{prefix}anim/main_{source_id:03}.anim");break
                except FileNotFoundError:continue
            if found is None:
                manifest["missing"].append(image_id);continue
            info,textures=anim(found)
            info["sourceImage"]=source_id
            info["textures"]={}
            for layer,(data,w,h) in textures.items():
                if data[:4]!=b"DDS ":continue
                name=f"image-{image_id}-{layer}.dds";(out/name).write_bytes(data)
                info["textures"][layer]={"file":name,"width":w,"height":h}
            manifest["images"][str(image_id)]=info
        (out/"manifest.json").write_text(json.dumps(manifest,separators=(",",":")))
        print(json.dumps({"images":len(manifest["images"]),"tiles":len(manifest["tiles"]),"missing":manifest["missing"],"out":str(out)}))
    finally:c.close()
if __name__=="__main__":main()
