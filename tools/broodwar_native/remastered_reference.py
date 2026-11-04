# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy", "pillow", "scikit-image"]
# ///
# Run: runtime/asset-venv/bin/python tools/broodwar_native/remastered_reference.py CAPTURE.json
"""Deterministic prelit HD compositor; retains explicit unsupported-state evidence."""
from dataclasses import dataclass
import hashlib
import json
import math
from pathlib import Path
import sys
from typing import TypedDict

import numpy as np
from PIL import Image

from client import RUNTIME


class NativeImage(TypedDict):
    id: int
    frame: int
    flags: int
    drawfunc: int
    mapX: int
    mapY: int
    offsetX: int
    offsetY: int


@dataclass(frozen=True, slots=True)
class Region:
    x: float
    y: float
    width: float
    height: float
    scale: float = 2
    elevation: float = 90


def team_pixels(diffuse: np.ndarray, mask: np.ndarray, color: tuple[int, int, int]) -> np.ndarray:
    """The team mask blends base RGB with base RGB multiplied by player RGB."""
    result = diffuse.copy()
    weight = mask[..., 0:1].astype(float)/255
    result[..., :3] = np.rint(diffuse[..., :3]*(1-weight+weight*np.asarray(color)/255)).astype(np.uint8)
    return result


def player_color(colors: list[list[int]], owner: int) -> tuple[int, int, int]:
    """An all-zero SCR initialization slot is absent, not authored black paint."""
    defaults = ((244,4,4),(12,72,204),(44,180,148),(136,64,156),
                (248,140,20),(112,48,20),(204,224,208),(252,252,56))
    if 0 <= owner < len(colors) and len(colors[owner]) >= 3 and any(colors[owner][:3]):
        return tuple(colors[owner][:3])
    return defaults[owner] if 0 <= owner < len(defaults) else (255,255,255)


class ReferenceRenderer:
    """Caches immutable owned atlases; each render starts from a new canvas."""

    def __init__(self, runtime: Path = RUNTIME) -> None:
        self.folder = runtime/'world-sprites'
        self.terrain = runtime/'scene-assets'
        self.catalog = json.loads((self.folder/'catalog.json').read_text())
        self.manifest = json.loads((self.terrain/'manifest.json').read_text())
        self.atlases: dict[str, Image.Image] = {}

    def texture(self, name: str) -> Image.Image:
        if name not in self.atlases:
            with Image.open(self.folder/name) as source:
                self.atlases[name] = source.convert('RGBA')
        return self.atlases[name]

    def sprite(self, native: NativeImage, color: tuple[int, int, int]) -> Image.Image:
        entry = self.catalog['images'][str(native['id'])]
        record = entry['frames'][native['frame']]
        box = (record['x'], record['y'], record['x']+record['w'], record['y']+record['h'])
        pixels = np.asarray(self.texture(entry['file']).crop(box))
        if 'teamFile' in entry and native['drawfunc'] != 10:
            mask = np.asarray(self.texture(entry['teamFile']).crop(box))
            pixels = team_pixels(pixels, mask, color)
        if native['drawfunc'] == 10:
            pixels = pixels.copy()
            pixels[..., :3] = 0
            pixels[..., 3] = np.rint(pixels[..., 3]*0.5).astype(np.uint8)
        image = Image.fromarray(pixels)
        if native['flags'] & 2:
            image = image.transpose(Image.Transpose.FLIP_LEFT_RIGHT)
        return image

    def render(self, snapshot: Path, region: Region) -> Path:
        document = json.loads(snapshot.read_text())
        state = document.get('state', document)
        world = self.manifest['map']
        if state['map']['path'].lower() != world['nativePath'].lower():
            raise RuntimeError('Reference snapshot and installed terrain refer to different maps')
        sine = math.sin(math.radians(region.elevation))
        canvas = Image.new('RGBA',(round(region.width*region.scale),round(region.height*region.scale)),(0,0,0,255))
        for ty in range(math.floor(region.y/32),math.ceil((region.y+region.height)/32)):
            for tx in range(math.floor(region.x/32),math.ceil((region.x+region.width)/32)):
                if not (0 <= tx < world['width'] and 0 <= ty < world['height']):
                    continue
                entry = self.manifest['tiles'][str(world['tiles'][ty*world['width']+tx])]
                size = (round(32*region.scale),round(32*region.scale))
                with Image.open(self.terrain/entry['file']) as tile:
                    terrain = tile.convert('RGBA').resize(size,Image.Resampling.LANCZOS)
                creep = set(document.get('creepTiles',[]))
                creep_art = self.catalog['creep'][world['tileset']]
                index = ty*world['width']+tx
                if index in creep:
                    variants = creep_art['tiles']
                    terrain = self.texture(variants[(tx*7+ty*13)%len(variants)]['file']).resize(size,Image.Resampling.LANCZOS)
                else:
                    neighbors = sum(1<<bit for bit,(dx,dy) in enumerate(((1,1),(0,1),(-1,1),(1,0),(-1,0),(1,-1),(0,-1),(-1,-1)))
                                    if 0<=tx+dx<world['width'] and 0<=ty+dy<world['height'] and (ty+dy)*world['width']+tx+dx in creep)
                    edge = creep_art.get('edgeLookup',[0]*256)[neighbors]
                    if edge:
                        terrain.alpha_composite(self.texture(creep_art['edges'][edge-1]).resize(size,Image.Resampling.LANCZOS))
                fog = document.get('fog')
                if fog is not None:
                    corners = []
                    for cx,cy in ((tx,ty),(tx+1,ty),(tx,ty+1),(tx+1,ty+1)):
                        values = [fog.get(str(y*world['width']+x),48) if 0<=x<world['width'] and 0<=y<world['height'] else 48
                                  for x,y in ((cx-1,cy-1),(cx,cy-1),(cx-1,cy),(cx,cy))]
                        corners.append(0 if 48 in values else sum(1 if value==50 else .35 for value in values)/4)
                    axis = (np.arange(size[0])+.5)/size[0]
                    weight = axis*axis*(3-2*axis)
                    a,b = np.meshgrid(weight,weight)
                    shade = (corners[0]*(1-a)+corners[1]*a)*(1-b)+(corners[2]*(1-a)+corners[3]*a)*b
                    rgba = np.asarray(terrain).copy()
                    rgba[...,:3] = np.rint(rgba[...,:3]*shade[...,None]).astype(np.uint8)
                    terrain = Image.fromarray(rgba)
                canvas.alpha_composite(terrain,(round((tx*32-region.x)*region.scale),round((ty*32-region.y)*region.scale)))
        canvas = canvas.resize((canvas.width,round(canvas.height*sine)),Image.Resampling.LANCZOS)
        missing = []
        draws = []
        sprites = state['units']+state.get('dyingUnits',[])+state.get('loneSprites',[])
        # Elevation then screen Y is the engine's coarse sprite ordering. Equal-key
        # order cannot be proven without the native draw-list tie breaker.
        sprites = sorted(enumerate(sprites),key=lambda pair:(pair[1].get('elevation',4 if pair[1].get('airborne') else 0),pair[1]['y'],pair[0]))
        for _,unit in sprites:
            if unit.get('renderHidden') or unit.get('transported'):
                continue
            chain = unit.get('visual')
            if not chain or chain.get('truncated'):
                missing.append({'unit':unit['id'],'reason':'missing or truncated native image chain'})
                continue
            owner = unit['owner']
            colors = state['playerColors']
            color = player_color(colors, owner)
            for native in reversed(chain['images']):
                if native['flags'] & 0x40:
                    continue
                entry = self.catalog['images'].get(str(native['id']))
                if entry is None or not 0 <= native['frame'] < len(entry['frames']):
                    missing.append({'image':native['id'],'frame':native['frame'],'reason':'missing HD frame'})
                    continue
                if native['drawfunc'] not in (0,9,10):
                    missing.append({'image':native['id'],'reason':f"unsupported draw function {native['drawfunc']}"})
                record = entry['frames'][native['frame']]
                image = self.sprite(native,color)
                scale = region.scale/entry['scale']
                image = image.resize((max(1,round(image.width*scale)),max(1,round(image.height*scale))),Image.Resampling.LANCZOS)
                # Native map position includes the classic frame's crop offset.
                # HD crop offsets can differ: use GRP canvas center and HD record.
                ox = entry['grpWidth']*entry['scale']-record['ox']-record['w'] if native['flags']&2 else record['ox']
                left = unit['x']-entry['grpWidth']//2+native['offsetX']+ox/entry['scale']
                top = unit['y']*sine-entry['grpHeight']//2+native['offsetY']+record['oy']/entry['scale']
                canvas.alpha_composite(image,(round((left-region.x)*region.scale),round((top-region.y*sine)*region.scale)))
                draws.append({'unit':unit['id'],'image':native['id'],'frame':native['frame'],'mirrored':bool(native['flags']&2),
                              'drawfunc':native['drawfunc'],'sourceSha256':entry['sourceSha256']})
        output = snapshot.with_name(snapshot.stem+'-reference.png')
        canvas.save(output)
        report = {'frame':state['frame'],'snapshotSha256':hashlib.sha256(snapshot.read_bytes()).hexdigest(),
                  'imageSha256':hashlib.sha256(output.read_bytes()).hexdigest(),'region':{'x':region.x,'y':region.y,'width':region.width,'height':region.height,'scale':region.scale,'elevation':region.elevation},
                  'draws':draws,'missing':missing,'clientEquivalent':False,
                  'limitations':['Fog/creep use captured viewer state when supplied; creep variant seed and fog smoothing are viewer approximations, not a verified client implementation.',
                                 'For elevation below 90 degrees the terrain and ground positions are projected to the requested view; this is a camera-adapted reference, not a client framebuffer.',
                                 'Terrain height displacement and ground-shadow perspective are not yet matched.',
                                 'Native sprite elevation and equal-depth draw-list tie breaker are not exported for units.',
                                 'Drawfunc 9 special color remapping is not reproduced.',
                                 'Prelit diffuse mode only; real-time lighting is excluded.',
                                 'Shadow opacity 0.5 is provisional; texture alpha encodes coverage, not the client blend strength.']}
        output.with_suffix('.json').write_text(json.dumps(report,indent=2)+'\n')
        return output


if __name__ == '__main__':
    source = Path(sys.argv[1])
    region = Region(*(int(value) for value in sys.argv[2:])) if len(sys.argv)>2 else Region(512,256,1024,768)
    print(ReferenceRenderer().render(source,region))
