"""Inventory installed HD layers and export lossless, opaque relief data privately."""
import argparse
import hashlib
import io
import json
from collections import Counter
from pathlib import Path

import numpy as np
from PIL import Image
from scipy.ndimage import distance_transform_edt
from client import RUNTIME
from map_assets import dependency
from remastered_layers import normals
from scene_assets import Casc, anim


def depth_from_normals(normal: np.ndarray, mask: np.ndarray) -> np.ndarray:
    """Least-squares periodic integration; only a fallback, not measured geometry."""
    n = normals(normal)
    gx = np.where(mask, -n[..., 0]/np.maximum(n[..., 2], .2), 0)
    gy = np.where(mask, n[..., 1]/np.maximum(n[..., 2], .2), 0)
    fy = 2*np.pi*np.fft.fftfreq(mask.shape[0])[:, None]
    fx = 2*np.pi*np.fft.fftfreq(mask.shape[1])[None, :]
    denominator = fx*fx+fy*fy
    denominator[0, 0] = 1
    z = np.fft.ifft2((-1j*fx*np.fft.fft2(gx)-1j*fy*np.fft.fft2(gy))/denominator).real
    if not mask.any():
        return np.zeros(mask.shape, np.uint8)
    lo, hi = np.percentile(z[mask], [2, 98])
    return np.clip((z-lo)/max(hi-lo, 1)*128, 0, 255).astype(np.uint8)


def pad_depth(depth: np.ndarray, mask: np.ndarray) -> np.ndarray:
    """Extend the nearest surface depth through transparent texels without moving the silhouette."""
    if not mask.any():
        return np.zeros_like(depth)
    nearest = distance_transform_edt(~mask, return_distances=False, return_indices=True)
    return depth[tuple(nearest)]


def prepare(snapshot: Path, out: Path, all_images: bool = False) -> None:
    out.mkdir(parents=True, exist_ok=True)
    folder = RUNTIME/'world-sprites'
    catalog = json.loads((folder/'catalog.json').read_text())
    capture = json.loads(snapshot.read_text())
    state = capture.get('state', capture)
    selected = set()
    for unit in state['units']:
        for native in unit.get('visual', {}).get('images', [unit['image']]):
            if native.get('drawfunc') != 10:
                selected.add(str(native['id']))
    sources = {row['sourceImage'] for key, row in catalog['images'].items() if all_images or key in selected}
    casc = Casc(dependency(RUNTIME, 'CascLib'), RUNTIME/'bottle/drive_c/Program Files (x86)/StarCraft')
    report = {'snapshot': str(snapshot), 'snapshotSha256': hashlib.sha256(snapshot.read_bytes()).hexdigest(),
              'images': {}, 'terrainProbes': {}, 'depthScale': '0.5 HD pixels per depth byte; provisional'}
    exports = {}
    try:
        for source in sorted({row['sourceImage'] for row in catalog['images'].values()}):
            path = f'anim/main_{source:03}.anim'
            try:
                data = casc.read(path)
            except FileNotFoundError:
                path = 'HD2/'+path
                data = casc.read(path)
            info, layers = anim(data)
            report['images'][str(source)] = {'path': path, 'sha256': hashlib.sha256(data).hexdigest(),
                                           'layers': list(layers), 'frames': len(info['frames'])}
            if source not in sources:
                continue
            decoded = {name: np.array(Image.open(io.BytesIO(value[0])).convert('RGBA')) for name, value in layers.items()}
            files = {}
            for name, pixels in decoded.items():
                if name not in ('bright','normal','specular','emissive','ao_depth'):
                    continue
                if name == 'normal':
                    pixels = np.rint((normals(pixels)+1)*127.5).clip(0,255).astype(np.uint8)
                elif name == 'ao_depth':
                    pixels = pixels[..., [3,1,3]].copy()
                    for record in info['frames']:
                        x,y,w,h = (record[k] for k in ('x','y','w','h'))
                        mask = decoded['diffuse'][y:y+h,x:x+w,3] > 0
                        pixels[y:y+h,x:x+w,0] = pad_depth(pixels[y:y+h,x:x+w,0], mask)
                else:
                    pixels = pixels[..., :3]
                filename = f'relief-{source}-{name}.png'
                Image.fromarray(pixels).save(folder/filename, compress_level=1)
                files[name] = filename
            depth_source = 'ao_depth.A'
            if 'ao_depth' not in decoded and 'normal' in decoded:
                depth = np.zeros(decoded['normal'].shape[:2], np.uint8)
                for frame in info['frames']:
                    x,y,w,h = (frame[k] for k in ('x','y','w','h'))
                    depth[y:y+h,x:x+w] = depth_from_normals(decoded['normal'][y:y+h,x:x+w], decoded['diffuse'][y:y+h,x:x+w,3]>127)
                filename = f'relief-{source}-ao_depth.png'
                Image.fromarray(np.stack((depth,np.full_like(depth,255),depth),axis=-1)).save(folder/filename)
                files['ao_depth'] = filename
                depth_source = 'normal integration; normalized scale'
            exports[source] = {'files': files, 'depthSource': depth_source if 'ao_depth' in files else None}
            report['images'][str(source)]['export'] = exports[source]
        for tileset in ('badlands','platform','install','ashworld','jungle','desert','ice','twilight'):
            for suffix in ('.dds.vr4','.normal.dds.vr4','.height.dds.vr4','.dds.vr4.normal','.dds.vr4.height'):
                path = 'tileset/'+tileset+suffix
                try:
                    data = casc.read(path)
                    report['terrainProbes'][path] = {'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()}
                except FileNotFoundError:
                    report['terrainProbes'][path] = None
        for row in catalog['images'].values():
            if row['sourceImage'] in exports:
                row['relief'] = exports[row['sourceImage']]
        catalog['reliefSchemaVersion'] = 1
        temporary = folder/'catalog.relief.tmp'
        temporary.write_text(json.dumps(catalog,separators=(',',':'))+'\n')
        temporary.replace(folder/'catalog.json')
        report['layerCounts'] = dict(Counter(layer for row in report['images'].values() for layer in row['layers']))
        report['selectedImages'] = sorted(selected, key=int)
        (out/'installed-layers.json').write_text(json.dumps(report,indent=2)+'\n')
        print(json.dumps({'sources':len(report['images']),'exported':len(exports),'layers':report['layerCounts']}))
    finally:
        casc.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('snapshot', type=Path)
    parser.add_argument('--all', action='store_true', dest='all_images')
    parser.add_argument('--out', type=Path, default=RUNTIME/'3d-proof/m9')
    args = parser.parse_args()
    prepare(args.snapshot, args.out, args.all_images)
