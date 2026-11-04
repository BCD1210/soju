# /// script
# requires-python = ">=3.11"
# dependencies = ["pillow"]
# ///
# Run: runtime/asset-venv/bin/python tools/broodwar_native/prepare_world_sprites.py
"""Export private native-image atlases and eight owned creep tile sets."""
import hashlib
import io
import json
import struct

from PIL import Image

from client import RUNTIME
from map_assets import dependency
from scene_assets import Casc, anim, ddsgrp


def prepare() -> None:
    """Keep original frame metadata; the native engine chooses animation frames."""
    folder = RUNTIME/'world-sprites'
    folder.mkdir(exist_ok=True)
    previous = json.loads((folder/'catalog.json').read_text()) if (folder/'catalog.json').exists() else {}
    casc = Casc(dependency(RUNTIME,'CascLib'),RUNTIME/'bottle/drive_c/Program Files (x86)/StarCraft')
    images = {}
    sources = {}
    missing = []
    creep = {}
    try:
        relations = casc.read('images.rel')
        image_dat = casc.read('arr/images.dat')
        image_names = casc.read('arr/images.tbl')
        for image in range(999):
            source = image
            seen = set()
            while source not in seen:
                seen.add(source)
                mode,target = struct.unpack_from('<II',relations,source*8)
                if mode != 0x200:
                    break
                source = target
            if source not in sources:
                data = None
                for prefix in ('','HD2/'):
                    try:
                        data = casc.read(f'{prefix}anim/main_{source:03}.anim')
                        break
                    except FileNotFoundError:
                        continue
                if data is None:
                    missing.append(image)
                    continue
                info,layers = anim(data)
                if not info['width'] or not info['height'] or 'diffuse' not in layers:
                    missing.append(image)
                    continue
                filename = f'atlas-{source}.png'
                with Image.open(io.BytesIO(layers['diffuse'][0])) as texture:
                    rgba = texture.convert('RGBA')
                    rgba.save(folder/filename,compress_level=1)
                    info.update(atlasWidth=rgba.width,atlasHeight=rgba.height)
                info.update(file=filename,sourceImage=source,sourceSha256=hashlib.sha256(data).hexdigest())
                if 'teamcolor' in layers:
                    team_file = f'team-{source}.png'
                    with Image.open(io.BytesIO(layers['teamcolor'][0])) as texture:
                        texture.convert('RGBA').save(folder/team_file,compress_level=1)
                    info['teamFile'] = team_file
                sources[source] = info
            entry = dict(sources[source])
            name_index = struct.unpack_from('<I',image_dat,image*4)[0]
            name_offset = struct.unpack_from('<H',image_names,name_index*2)[0]
            name = image_names[name_offset:].split(b'\0',1)[0].decode('ascii')
            grp = casc.read('unit/'+name)
            _,entry['grpWidth'],entry['grpHeight'] = struct.unpack_from('<HHH',grp)
            images[str(image)] = entry
            if image%100==0:
                print(f'Exported image {image}',flush=True)
        for tileset in ('badlands','platform','install','ashworld','jungle','desert','ice','twilight'):
            cv5 = casc.read(f'tileset/{tileset}.cv5')
            graphics = casc.read(f'tileset/{tileset}.dds.vr4')
            tiles = ddsgrp(graphics)
            entries = []
            for variant in range(16):
                index = struct.unpack_from('<H',cv5,52+20+variant*2)[0]
                if index == 0:
                    continue
                filename = f'creep-{tileset}-{variant}.png'
                with Image.open(io.BytesIO(tiles[index][0])) as texture:
                    texture.convert('RGBA').save(folder/filename)
                entries.append({'file':filename,'megatile':index})
            creep[tileset] = {**previous.get('creep',{}).get(tileset,{}),
                             'tiles':entries,'sourceSha256':hashlib.sha256(graphics).hexdigest(),'cv5Group':1}
        catalog = {'schemaVersion':1,'source':'owned-remastered-casc','images':images,'creep':creep,'missing':missing,
                   'timing':'Native image.frame only; no guessed iscript duration',
                   'layer':'diffuse: original prelit color and alpha'}
        (folder/'catalog.json').write_text(json.dumps(catalog,separators=(',',':'))+'\n')
        print(json.dumps({'images':len(images),'atlases':len(sources),'tilesets':len(creep),'missing':missing}),flush=True)
    finally:
        casc.close()


if __name__ == '__main__':
    prepare()
