"""Measure changed hue on paired, identically posed model renders (not background)."""
import argparse
import json
from pathlib import Path
import numpy as np
from PIL import Image


def measure(folder, kind):
    red = np.asarray(Image.open(folder / f'{kind}-player-1.png').convert('RGB'), dtype=float) / 255
    neutral_image = Image.open(folder / f'{kind}-player-9.png').convert('RGB')
    neutral = np.asarray(neutral_image, dtype=float) / 255
    background = neutral[0, 0]
    foreground = np.max(abs(neutral-background), axis=2) > 0.025
    a = np.asarray(Image.fromarray(np.uint8(red*255)).convert('HSV'), dtype=float)/255
    b = np.asarray(neutral_image.convert('HSV'), dtype=float)/255
    delta = abs(a[:, :, 0]-b[:, :, 0]);delta = np.minimum(delta, 1-delta)
    changed = foreground & (np.max(abs(red-neutral), axis=2) > 0.025) & ((delta > 0.025) | (abs(a[:, :, 1]-b[:, :, 1]) > 0.08))
    count = int(foreground.sum())
    fraction = float(changed.sum()/max(1,count))
    return {'type':kind,'modelPixels':count,'hueChangedPixels':int(changed.sum()),'fraction':fraction,'passed':count>100 and 0<fraction<0.35}


if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('folder',type=Path)
    parser.add_argument('--types',default='0,7')
    parser.add_argument('--record-only',action='store_true')
    args=parser.parse_args()
    rows=[measure(args.folder,int(kind)) for kind in args.types.split(',')]
    report={'passed':all(r['passed'] for r in rows),'maximumFraction':0.35,'models':rows}
    (args.folder/'team-colour-check.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))
    if not args.record_only and not report['passed']:raise SystemExit(1)
