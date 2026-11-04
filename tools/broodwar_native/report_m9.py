"""Bind final private stereo captures, scores and binary hashes into one report."""
import hashlib
import json
from pathlib import Path
import numpy as np
from PIL import Image, ImageDraw
from client import RUNTIME


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def report() -> None:
    root=RUNTIME/'3d-proof/m9'
    scores=json.loads((root/'canonical-depth/frame-00003-scores.json').read_text())
    angles=[]
    for angle in ('ne','nw','sw','se'):
        folder=root/f'final-{angle}'
        frames=sorted(p for p in folder.glob('frame-*.json') if 'right' not in p.name)
        candidates=[p for p in frames if p.with_name(p.stem+'-right.json').exists() and p.with_name(p.stem+'-right.png').exists()]
        left=candidates[-2] if len(candidates)>1 else candidates[-1]
        right=left.with_name(left.stem+'-right.json')
        a,b=json.loads(left.read_text()),json.loads(right.read_text())
        layers=[layer for unit in a['billboards'].values() for layer in unit['layers'] if not layer['shadow']]
        angles.append({'angle':angle,'left':str(left.with_suffix('.png')),'right':str(right.with_suffix('.png')),
            'leftSha256':digest(left.with_suffix('.png')),'rightSha256':digest(right.with_suffix('.png')),
            'sameXRFrame':a['xrFrame']==b['xrFrame'],'sameHeadSelectedLayers':a['billboards']==b['billboards'],
            'eyeSeparationWorld':float(np.linalg.norm(np.array(a['camera'])-b['camera'])),
            'worldUnitsPerMeter':a['worldUnitsPerMeter'],'camera':a['camera'],'orientation':a['orientation'],
            'reliefLayers':sum(bool(layer['relief']) for layer in layers),'nonShadowLayers':len(layers)})
    sheet=Image.new('RGB',(1200,1400),(20,20,20))
    draw=ImageDraw.Draw(sheet)
    for i,angle in enumerate(angles):
        with Image.open(angle['left']) as source:
            image=source.convert('RGB');image.thumbnail((600,665))
            x=(i%2)*600;y=(i//2)*700
            sheet.paste(image,(x+(600-image.width)//2,y+30))
            draw.text((x+12,y+10),angle['angle'].upper()+" | "+Path(angle['left']).name,fill='white')
    sheet.save(root/'xr-four-angles.png')
    metrics=json.loads((root/'eight-player-metrics.json').read_text())
    document={'canonicalScores':scores,'angles':angles,'performance':metrics,
        'visualSourceCommit':'2c2e85e','measurementBinaries':{name:digest(RUNTIME/name) for name in ('soju-bw-scene','soju-bw-xr')},
        'layerAudit':str(root/'installed-layers.json'),'typeAudit':str(root/'visible-type-audit.json'),
        'acceptance':{'canonicalRegions':scores['regionalSSIMPassed'],'alignment':scores['spriteAlignment']['passed'],
            'fourStereoViews':all(a['sameXRFrame'] and a['sameHeadSelectedLayers'] for a in angles),
            'terrainWithoutStretch':False,'ninetyHzP95':metrics['frameIntervals']['p95Ms']<=1000/90},
        'limits':['Relief does not recover hidden sides or measured metric depth.',
                  'Original cliff artwork still stretches at low angles; terrain acceptance remains open.',
                  'Lighting residual preserves the prelit reference; sun calibration remains approximate.',
                  'The performance load is frozen rendering data, not eight-player live simulation.']}
    (root/'acceptance.json').write_text(json.dumps(document,indent=2)+'\n')
    lines=['# M9 canonical comparison','', '| Region | SSIM | Pass >= 0.85 |','| --- | ---: | --- |']
    for row in scores['regions']:lines.append(f"| {row['region']} | {row['SSIM']:.6f} | {'yes' if row['SSIM']>=.85 else 'no'} |")
    lines += ['',f"Maximum crop-origin error: {scores['spriteAlignment']['maximumPixelError']:.6f} px.",
              'Reference: independent HD compositor with the explicitly corrected missing-player-color fallback. Not an owned client framebuffer.']
    (root/'canonical-comparison.md').write_text('\n'.join(lines)+'\n')
    print(json.dumps(document['acceptance'],indent=2))


if __name__=='__main__':report()
