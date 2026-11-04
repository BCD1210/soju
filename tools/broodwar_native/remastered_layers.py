"""Audit owned HD layers without changing the active terrain or model releases."""
import argparse
import hashlib
import json
import pathlib
import numpy as np
from PIL import Image
from client import RUNTIME


def source_manifest():
    candidates = [RUNTIME / 'scene-assets/manifest.json', *sorted(RUNTIME.glob('terrain-releases/*/manifest.json'))]
    return next(p for p in candidates if '239' in json.loads(p.read_text()).get('images', {}))


def layers(source, image):
    info = json.loads(source.read_text())['images'][str(image)]
    return info, {k: np.asarray(Image.open(source.parent / v['file']).convert('RGBA')) for k, v in info['textures'].items()}


def frame(info, atlas, index):
    f = info['frames'][index]
    result = np.zeros((info['height'], info['width'], 4), dtype=np.uint8)
    crop = atlas[f['y']:f['y'] + f['h'], f['x']:f['x'] + f['w']]
    x, y = f['ox'], f['oy']
    x0, y0 = max(0, x), max(0, y)
    x1, y1 = min(info['width'], x + f['w']), min(info['height'], y + f['h'])
    result[y0:y1, x0:x1] = crop[y0-y:y1-y, x0-x:x1-x]
    return result


def normals(pixels):
    xy = pixels[..., [0, 3]].astype(float) / 127.5 - 1
    return np.dstack((xy, np.sqrt(np.maximum(0, 1 - np.sum(xy*xy, axis=-1)))))


def audit(source, out):
    out.mkdir(parents=True, exist_ok=True)
    manifest = json.loads(source.read_text())
    report = {'manifest': str(source), 'manifestSha256': hashlib.sha256(source.read_bytes()).hexdigest(), 'images': {}}
    for image in manifest['images']:
        info, atlases = layers(source, image)
        count = min(17, len(info['frames']))
        stats = {}
        for name, atlas in atlases.items():
            samples = []
            for i in range(count):
                pixels = frame(info, atlas, i)
                mask = frame(info, atlases['diffuse'], i)[..., 3] > 127
                samples.append(pixels[mask])
                if i == 0:
                    Image.fromarray(pixels).save(out / f'{image}-{name}-frame0.png')
                    if name in ('normal', 'ao_depth'):
                        for c in range(4):
                            Image.fromarray(pixels[..., c]).save(out / f'{image}-{name}-{c}-frame0.png')
            values = np.concatenate(samples)
            stats[name] = {'sha256': hashlib.sha256((source.parent / info['textures'][name]['file']).read_bytes()).hexdigest(),
                           'channels': [{'min': int(v.min()), 'max': int(v.max()), 'mean': float(v.mean()), 'std': float(v.std())} for v in values.T]}
        report['images'][image] = {'frameCount': len(info['frames']), 'sampledFrames': count, 'canvas': [info['width'], info['height']], 'layers': stats}
        if 'normal' in atlases:
            n = normals(frame(info, atlases['normal'], 0))
            rgba = frame(info, atlases['diffuse'], 0)
            select = (rgba[..., 3] > 240) & (abs(n[..., 0]) < .08) & (n[..., 1] > .5) & (n[..., 1] < .95)
            hist, edges = np.histogram(n[..., 1][select], bins=90, range=(.5, .95))
            mode = float((edges[hist.argmax()] + edges[hist.argmax()+1])/2)
            report['images'][image]['upSurfaceElevationCandidate'] = float(np.degrees(np.arccos(mode)))
            report['images'][image]['upSurfaceSamples'] = int(select.sum())
    (out / 'layers.json').write_text(json.dumps(report, indent=2) + '\n')
    return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=pathlib.Path, default=source_manifest())
    parser.add_argument('--out', type=pathlib.Path, default=RUNTIME / '3d-proof/rm')
    args = parser.parse_args()
    audit(args.source, args.out)
    print(args.out / 'layers.json')
