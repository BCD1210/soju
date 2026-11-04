# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy", "pillow", "scikit-image"]
# ///
# Run: python3 scripts/broodwar-native.py assets-review /absolute/delivery
"""Read-only delivery intake; activate only after every measured gate passes."""
import datetime
import hashlib
import json
import pathlib
import shutil
import subprocess
from dataclasses import dataclass
from typing import Final

import numpy as np
from PIL import Image, ImageDraw

from client import RUNTIME
from model_assets import bundle, contained, inspect_glb, install
from remastered_validate import metrics

HERE: Final = pathlib.Path(__file__).resolve().parent
CALIBRATIONS: Final = {0: 'rigged-e32', 7: 'rigged-dense'}


@dataclass(frozen=True, slots=True)
class ReviewResult:
    passed: bool
    report: pathlib.Path


def review(source: pathlib.Path) -> ReviewResult:
    """Stage an immutable copy so a later delivery edit cannot bypass the gate."""
    source = source.expanduser().resolve()
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')
    out = RUNTIME/'3d-proof/m3/intake'/stamp
    stage = out/'delivery'
    shutil.copytree(source, stage, symlinks=False)
    models = bundle(stage)
    contract_path = stage/'delivery.json'
    contract = json.loads(contract_path.read_text()) if contract_path.exists() else {}
    rows = []
    for kind, (definition, model, info) in models.items():
        folder = out/kind
        folder.mkdir()
        failures = []
        declared = contract.get('models', {}).get(kind, {})
        if declared.get('up') != '+Y' or declared.get('forward') != '+Z':
            failures.append('Declare glTF +Y up and +Z forward in delivery.json; orientation needs visual review')
        if not model.with_suffix('.blend').is_file():
            failures.append('Editable .blend source matching GLB filename is required')
        if definition.get('teamMask', 'none') not in ('alpha', 'texture'):
            failures.append('Declare a team mask channel in catalog.json')
        if not definition.get('targetHeight'):
            failures.append('Declare targetHeight in terrain-tile scene units')
        if not (stage/'provenance.json').is_file():
            failures.append('provenance.json is required')
        if not declared.get('skeletonMapping'):
            failures.append('Declare skeletonMapping: native clip-driven rig or explicit named-bone mapping')
        lod_name = declared.get('lod1')
        if not lod_name:
            failures.append('LOD1 GLB is required')
        else:
            lod = inspect_glb(contained(stage, lod_name))
            if lod['triangles'] > 15000 or lod['triangles'] >= info['triangles']:
                failures.append('LOD1 must be smaller than LOD0 and at most 15000 triangles')
        if not 15000 <= info['triangles'] <= 40000:
            failures.append('LOD0 must contain 15000–40000 triangles')
        calibration = CALIBRATIONS.get(int(kind))
        if calibration is None:
            failures.append('No measured 17-direction calibration for this type; cannot approve')
            rows.append({'type': kind, 'failures': failures, 'passed': False})
            continue
        reference = RUNTIME/'models-remastered'/kind/calibration
        camera_document = json.loads((reference/'camera.json').read_text())
        source_geometry = np.load(reference/'source.npz')['vertices']
        aliases = definition.get('animations', {})
        idle = aliases.get('idle', ['Stand_full'])[0]
        job = {'model': str(model), 'idle': idle, 'camera': camera_document['camera'],
               'referenceHeight': float(np.ptp(source_geometry[:,2])),
               'referenceFoot': float(source_geometry[:,2].min()),
               'cameraLimitations': camera_document.get('limitations', []),
               'placement': 'Single height normalization and foot translation, shared by all 17 directions; no image registration'}
        job_path = folder/'render-job.json'
        job_path.write_text(json.dumps(job, indent=2)+'\n')
        with (folder/'blender.log').open('w') as log:
            subprocess.run([str(RUNTIME/'blender/Blender.app/Contents/MacOS/Blender'), '--background',
                            '--factory-startup', '--disable-autoexec', '--python', str(HERE/'delivery_render.py'),
                            '--', str(job_path)], stdout=log, stderr=subprocess.STDOUT, check=True)
        geometry = json.loads((folder/'geometry.json').read_text())
        low, high = geometry['blenderBounds']
        if abs(low[2]) > (high[2]-low[2])*.02:
            failures.append('Authored foot pivot exceeds 2% of model height')
        if not geometry['textures'] or any(not all(1 <= s <= 4096 for s in t['size']) for t in geometry['textures']):
            failures.append('Embedded texture dimensions must be 1–4096 pixels')
        mapping = declared.get('skeletonMapping')
        if mapping and mapping != 'native':
            if not isinstance(mapping, dict) or not all(name in geometry['bones'] for name in mapping.values()):
                failures.append('Skeleton mapping destinations must name delivered bones')
        if not geometry['bones'] or len(set(geometry['bones'])) != len(geometry['bones']):
            failures.append('Require a skeleton with unique bone names')
        for action in definition.get('requiredAnimations', ['idle', 'walk', 'attack']):
            if not any(a in geometry['clips'] for a in aliases.get(action, [])):
                failures.append(f'Missing explicit clip mapping: {action}')
        comparisons = folder/'comparisons'
        comparisons.mkdir()
        directions = []
        for d in range(17):
            with Image.open(reference/'references'/f'{d:02}-diffuse.png') as image:
                ref = image.convert('RGBA')
            with Image.open(folder/'renders'/f'{d:02}.png') as image:
                rendered = image.convert('RGBA')
            result = metrics(np.asarray(ref)/255., np.asarray(rendered)/255.)
            directions.append({'direction': d, **result})
            panel = Image.new('RGB', (ref.width*2, ref.height+28), '#181818')
            panel.paste(ref, (0,28), ref)
            panel.paste(rendered, (ref.width,28), rendered)
            ImageDraw.Draw(panel).text((4,4), f"Reference | delivery  IoU {result['silhouetteIoU']:.3f} SSIM {result['diffuseSSIM']:.3f}")
            panel.save(comparisons/f'{d:02}.png')
        if not all(d['passed'] for d in directions):
            failures.append('Every direction must meet IoU .95 and foreground SSIM .85')
        frames = []
        for path in sorted((folder/'turntable').glob('*.png')):
            with Image.open(path) as image:
                frames.append(image.convert('RGB'))
        frames[0].save(folder/'closeup.gif', save_all=True, append_images=frames[1:], duration=100, loop=0)
        proof = subprocess.run([str(RUNTIME/'soju-bw-scene'), '--model-proof', '--model-dir', str(stage),
                                '--catalog', str(stage/'catalog.json'), '--model-types', kind,
                                '--proof-dir', str(folder/'native')], capture_output=True, text=True)
        (folder/'native.log').write_text(proof.stdout+proof.stderr)
        if proof.returncode:
            failures.append('Native renderer rig/clip checks failed; see native.log')
        rows.append({'type': kind, 'passed': not failures, 'failures': failures, 'geometry': geometry,
                     'camera': job, 'directions': directions, 'source': info})
    passed = bool(rows) and all(row['passed'] for row in rows)
    report = {'passed': passed, 'source': str(source), 'models': rows,
              'physicalHeadsetVerified': False,
              'closeupProtocol': '36 perspective views, 50mm lens, neutral area light; desktop VR-distance proxy, not headset evidence',
              'orientationProtocol': 'Axis declaration plus measured bounds and 17 views; semantic forward requires artist review',
              'sourceHashes': {str(p.relative_to(stage)): hashlib.sha256(p.read_bytes()).hexdigest() for p in stage.rglob('*') if p.is_file()}}
    if passed:
        report['release'] = install(stage)
    path = out/'review.json'
    path.write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps({'passed': passed, 'report': str(path)}, indent=2))
    return ReviewResult(passed, path)
