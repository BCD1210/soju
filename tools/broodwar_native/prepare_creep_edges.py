"""Export owned HD creep borders and their eight-neighbor frame lookup."""
import io
import json
import struct
from PIL import Image
from client import RUNTIME
from map_assets import dependency
from scene_assets import Casc, ddsgrp


def frame_lookup():
    # Frame ordering documented by OpenBW ui/ui.h, load_image_data/draw_tiles:
    # https://github.com/OpenBW/openbw/blob/master/ui/ui.h
    # Neighbor bits: SE, S, SW, E, W, NE, N, NW.
    rules=((2,16),(8,36),(16,9),(64,2),(192,1),(96,4),(3,32),(6,8))
    shapes=[]
    for neighbors in range(256):
        shape=0
        for required,bits in rules:
            if neighbors & required == required:shape |= bits
        if shape & 33 == 33 or shape & 12 == 12:shape |= 64
        shapes.append(shape)
    ordered=sorted(set(shapes))
    return [ordered.index(shape) for shape in shapes]


def main():
    folder=RUNTIME/'world-sprites'
    catalog=json.loads((folder/'catalog.json').read_text())
    casc=Casc(dependency(RUNTIME,'CascLib'),RUNTIME/'bottle/drive_c/Program Files (x86)/StarCraft')
    lookup=frame_lookup()
    try:
        for name,entry in catalog['creep'].items():
            grp=casc.read(f'tileset/{name}.grp')
            hd=casc.read(f'tileset/{name}.dds.grp');textures=ddsgrp(hd);scale=hd[6]&15
            count,width,height=struct.unpack_from('<3H',grp)
            if count!=len(textures) or max(lookup)!=count:raise ValueError('Creep frame table mismatch')
            files=[]
            for index,(dds,w,h) in enumerate(textures):
                x,y,sw,sh,_=struct.unpack_from('<4BI',grp,6+index*8)
                if w>width*scale or h>height*scale or (x and x+sw!=width) or (y and y+sh!=height):
                    raise ValueError('Unsupported creep edge anchor')
                # HD silhouettes can extend past the classic crop. Preserve the
                # touching tile edge instead of stretching to the classic size.
                ox=width*scale-w if x else 0;oy=height*scale-h if y else 0
                canvas=Image.new('RGBA',(width*scale,height*scale))
                with Image.open(io.BytesIO(dds)) as image:canvas.paste(image.convert('RGBA'),(ox,oy))
                filename=f'creep-edge-{name}-{index}.png';canvas.save(folder/filename);files.append(filename)
            entry['edges']=files;entry['edgeLookup']=lookup
        temporary=folder/'catalog.json.new';temporary.write_text(json.dumps(catalog,separators=(',',':'))+'\n')
        temporary.replace(folder/'catalog.json')
        print(json.dumps({name:len(entry['edges']) for name,entry in catalog['creep'].items()}))
    finally:casc.close()


if __name__=='__main__':main()
