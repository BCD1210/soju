# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy", "pillow", "scikit-image"]
# ///
# Run: runtime/asset-venv/bin/python tools/broodwar_native/model_fidelity_report.py
"""Measure installed placements and publish a hash-bound live fallback decision."""
import hashlib
import json
import struct
import subprocess
import sys
from pathlib import Path

import numpy as np
from PIL import Image

from client import RUNTIME
from remastered_layers import frame
from remastered_validate import metrics
from scene_assets import Casc
from map_assets import dependency


def review_installed() -> None:
    """Build jobs from installed DAT relations, then measure the actual viewer."""
    proof = RUNTIME/'3d-proof/m7'
    proof.mkdir(parents=True,exist_ok=True)
    casc = Casc(dependency(RUNTIME,'CascLib'),RUNTIME/'bottle/drive_c/Program Files (x86)/StarCraft')
    try:
        units = casc.read('arr/units.dat')
        flingy = casc.read('arr/flingy.dat')
        sprites = casc.read('arr/sprites.dat')
        images = casc.read('arr/images.dat')
    finally:
        casc.close()
    catalog = json.loads((RUNTIME/'world-sprites/catalog.json').read_text())['images']
    rows = []
    for kind in range(228):
        sprite = struct.unpack_from('<H',flingy,units[kind]*2)[0]
        image = struct.unpack_from('<H',sprites,sprite*2)[0]
        entry = catalog.get(str(image))
        if entry is None:
            continue
        rows.append({'type':kind,'image':image,**{key:entry[key] for key in ('width','height','scale','grpWidth','grpHeight')},
                     'directions':min(len(entry['frames']),17) if images[999*4+image] else 1})
    job = proof/'fidelity-job.json'
    job.write_text(json.dumps({'models':rows,'output':str(proof/'model-renders')},indent=2)+'\n')
    with (proof/'fidelity-render.log').open('w') as log:
        subprocess.run([str(RUNTIME/'soju-bw-scene'),'--fidelity-job',str(job)],stdout=log,stderr=subprocess.STDOUT,check=True)
    report()


def report() -> None:
    proof = RUNTIME/'3d-proof/m7'
    job = json.loads((proof/'fidelity-job.json').read_text())
    catalog = json.loads((RUNTIME/'world-sprites/catalog.json').read_text())['images']
    rendered = {row['type']:row for row in json.loads((proof/'model-renders/renders.json').read_text())['models']}
    sources = {row['type']:row for row in job['models']}
    root = Path(__file__).resolve().parents[2]
    names = json.loads((root/'resources/broodwar-unit-types.json').read_text())['types']
    rows = []
    for kind in range(228):
        row = {'type':kind,'name':names[str(kind)],'passed':False,'status':'unmeasured','minimumIoU':None,'minimumSSIM':None}
        result = rendered.get(kind,{})
        if 'files' in result and kind in sources:
            entry = catalog[str(sources[kind]['image'])]
            with Image.open(RUNTIME/'world-sprites'/entry['file']) as atlas:
                rgba = np.asarray(atlas.convert('RGBA'))
            views = []
            for direction,filename in enumerate(result['files']):
                reference = frame(entry,rgba,direction)/255.
                with Image.open(proof/'model-renders'/filename) as image:
                    candidate = np.asarray(image.convert('RGBA'))/255.
                views.append({'direction':direction,**metrics(reference,candidate)})
            model = RUNTIME/'models-3d'/result['model']
            row.update(minimumIoU=min(v['silhouetteIoU'] for v in views),minimumSSIM=min(v['diffuseSSIM'] for v in views),
                       passed=all(v['passed'] for v in views),directions=views,modelSha256=hashlib.sha256(model.read_bytes()).hexdigest(),
                       file=result['model'],modelRevision=result.get('revision'),sourceSha256=entry['sourceSha256'])
            row['status'] = 'passed' if row['passed'] else 'failed'
        else:
            row['reason'] = result.get('error','No owned HD source metadata')
        rows.append(row)
    document = {'models':rows,'thresholds':{'IoU':.95,'SSIM':.85},
                'protocol':'Native SceneKit, installed scale/pivot, white team, fixed 32-degree orthographic camera, transparent RGBA, no registration or resizing; minimum over 17 directional candidates or one static building frame.',
                'limitations':['First directional frame group is a candidate idle pose, not a verified action mapping for every type.',
                               'Non-directional buildings have one view; no hidden-side quality claim.',
                               'Frequency tiers are an explicit prioritization estimate, not measured match telemetry.']}
    (proof/'model-fidelity.json').write_text(json.dumps(document,indent=2)+'\n')
    # Keep the runtime gate small. It checks the installed GLB hash before approval.
    gate = {'models':[{k:v for k,v in row.items() if k in ('type','passed','minimumIoU','minimumSSIM','modelSha256','modelRevision')} for row in rows]}
    temporary = RUNTIME/'model-fidelity.json.new'
    temporary.write_text(json.dumps(gate)+'\n')
    temporary.replace(RUNTIME/'model-fidelity.json')
    common = [7,41,64,35,0,37,38,65,66,42,106,131,154,109,156,111,160,5,30,2,34,43,103,61]
    regular = [110,149,157,112,113,114,122,123,124,125,135,139,142,143,144,146,132,133,155,163,164,166,167,169,170,171,172,1,3,8,9,11,12,39,44,45,46,47,58,60,62,63,67,68,69,70,71,72,83,84]
    def priority(row) -> tuple[int,int]:
        kind = row['type']
        if kind in common:
            return (0,common.index(kind))
        if kind in regular:
            return (1,regular.index(kind))
        return (2,kind)
    lines = ['# Brood War model queue','',
             'All 228 units.dat entries are accounted for. Failed and unmeasured entries use owned HD image chains in live desktop/XR rendering.',
             'Order: estimated normal-match frequency (workers/economy/basic armies, regular tech, then campaign/neutral/subunits). No match-frequency dataset was supplied; this is a priority estimate.',
             'Scores measure the installed model, including its placement. They are not artist-delivery approval. Private captures and per-view measurements: `runtime/3d-proof/m7/model-renders/` and `model-fidelity.json`.',
             'The first directional group is a candidate pose. Static buildings have one view. Missing scores remain unmeasured, never zero or a pass.','',
             '| Type | Unit/building | Status | Min IoU | Min SSIM | Live presentation |',
             '| ---: | --- | --- | ---: | ---: | --- |']
    for row in sorted(rows,key=priority):
        if row['passed']:
            continue
        iou = '—' if row['minimumIoU'] is None else f"{row['minimumIoU']:.4f}"
        ssim = '—' if row['minimumSSIM'] is None else f"{row['minimumSSIM']:.4f}"
        lines.append(f"| {row['type']} | {row['name']} | {row['status']} | {iou} | {ssim} | HD sprite |")
    (root/'docs/BROODWAR-MODEL-QUEUE.md').write_text('\n'.join(lines)+'\n')
    print(json.dumps({status:sum(row['status']==status for row in rows) for status in ('passed','failed','unmeasured')}))


if __name__ == '__main__':
    if '--installed' in sys.argv:
        review_installed()
    else:
        report()
