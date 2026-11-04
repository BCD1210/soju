# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy", "pillow"]
# ///
# Run with runtime/asset-venv/bin/python tools/broodwar_native/prepare_effect_sprites.py.
"""Export weapon graphics from the owned DAT/ANIM files, never from unit models."""
import hashlib
import io
import json
import pathlib
import struct

import numpy as np
from PIL import Image

from client import RUNTIME
from map_assets import dependency
from remastered_layers import frame
from scene_assets import Casc, anim


def prepare() -> None:
    out = RUNTIME/'effect-sprites'
    out.mkdir(exist_ok=True)
    casc = Casc(dependency(RUNTIME, 'CascLib'), RUNTIME/'bottle/drive_c/Program Files (x86)/StarCraft')
    try:
        weapons = casc.read('arr/weapons.dat')
        flingy = casc.read('arr/flingy.dat')
        sprites = casc.read('arr/sprites.dat')
        relations = casc.read('images.rel')
        mapping = {}
        images = {}
        missing = []
        for weapon in range(130):
            graphic = struct.unpack_from('<I', weapons, 260+weapon*4)[0]
            if not 0 < graphic < 209:
                continue
            sprite = struct.unpack_from('<H', flingy, graphic*2)[0]
            image = struct.unpack_from('<H', sprites, sprite*2)[0]
            mapping[str(weapon)] = image
        for image in sorted(set(mapping.values())):
            source = image
            seen = set()
            while source not in seen:
                seen.add(source)
                mode, target = struct.unpack_from('<II', relations, source*8)
                if mode != 0x200:
                    break
                source = target
            data = None
            for prefix in ('', 'HD2/'):
                try:
                    data = casc.read(f'{prefix}anim/main_{source:03}.anim')
                    break
                except FileNotFoundError:
                    continue
            if data is None:
                missing.append(image)
                continue
            info, layers = anim(data)
            if not info['width'] or not info['height']:
                missing.append(image)
                continue
            layer = 'bright' if 'bright' in layers else 'diffuse'
            with Image.open(io.BytesIO(layers[layer][0])) as texture:
                atlas = np.asarray(texture.convert('RGBA'))
            # Bright is illumination, diffuse owns alpha coverage.
            with Image.open(io.BytesIO(layers['diffuse'][0])) as texture:
                alpha = np.asarray(texture.convert('RGBA'))[...,3]
            atlas = atlas.copy()
            atlas[...,3] = alpha
            folder = out/str(image)
            folder.mkdir(exist_ok=True)
            for index in range(len(info['frames'])):
                Image.fromarray(frame(info, atlas, index)).save(folder/f'{index}.png')
            images[str(image)] = {'width': info['width'], 'height': info['height'], 'scale': info['scale'],
                                  'frames': len(info['frames']), 'sourceImage': source,
                                  'sourceSha256': hashlib.sha256(data).hexdigest(), 'layer': layer}
        report = {'schemaVersion': 1, 'source': 'owned-remastered-casc', 'weapons': mapping,
                  'images': images, 'missing': missing,
                  'timing': 'Native bullet image.frame selects the frame; no guessed simulation timing',
                  'limitations': ['Weapon main graphics only; secondary iscript-spawned effects require native image-chain capture']}
        (out/'catalog.json').write_text(json.dumps(report, indent=2)+'\n')
        print(json.dumps({'weapons': len(mapping), 'images': len(images), 'missing': missing}))
    finally:
        casc.close()


if __name__ == '__main__':
    prepare()
