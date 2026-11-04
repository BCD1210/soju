"""Summarize private M10 timing and canonical evidence without copying game assets."""
import hashlib
import json
import numpy as np
from PIL import Image, ImageDraw
from pathlib import Path
from client import RUNTIME


def report():
    root = RUNTIME / '3d-proof/m10'
    runs = [('SceneKit baseline', 'baseline-metrics.json'),
            ('Two-eye batches, capture tooling enabled', 'batched-metrics.json'),
            ('Amplified stereo, 4x MSAA, relief LOD', 'lod-stationary-metrics.json'),
            ('Amplified stereo, 2x MSAA, occlusion, specialized shaders', 'specialized-2x-metrics.json'),
            ('Amplified stereo, 1x sampling, runtime pacing', 'final-snapshot-metrics.json')]
    lines = ['# M10 frozen 1,728-object scene', '',
             'All times are milliseconds. CPU and GPU overlap; do not add them.', '',
             '| Stage | Frame mean / p95 | Stereo CPU mean / p95 | Stereo GPU mean / p95 |',
             '| --- | ---: | ---: | ---: |']
    documents = []
    for name, file in runs:
        data = json.loads((root / file).read_text())
        documents.append({'name': name, 'file': file, 'metrics': data})
        cells = [' / '.join(f'{data[key][stat]:.3f}' for stat in ('meanMs', 'p95Ms'))
                 for key in ('frameIntervals', 'stereoCPUEncode', 'stereoGPU')]
        lines.append('| ' + ' | '.join([name, *cells]) + ' |')
    lines += ['', '## CPU breakdown', '', '| Run | Stage | Mean | p95 |', '| --- | --- | ---: | ---: |']
    for doc in (documents[0], documents[-1]):
        for name, timing in doc['metrics'].get('stagesCPU', {}).items():
            lines.append(f"| {doc['name']} | {name} | {timing['meanMs']:.3f} | {timing['p95Ms']:.3f} |")
    lines += ['', 'Eye-stage measurements are per eye; billboard update is per stereo frame.',
              'Intermediate runs precede corrected texture origins and include capture/tooling and connection transitions; they are profiling diagnostics. Final run disables PNG and Metal capture.',
              'Simulator transport/encode and normal background applications remain active. This is frozen rendering, not live simulation.']
    (root / 'timings.md').write_text('\n'.join(lines) + '\n')
    scores = json.loads((root / 'canonical-final/frame-00003-scores.json').read_text())
    final = documents[-1]['metrics']
    angles = []
    sheet = Image.new('RGB', (1200, 1400), (20, 20, 20))
    draw = ImageDraw.Draw(sheet)
    for index, angle in enumerate(('ne', 'nw', 'sw', 'se')):
        left = root / f'final-{angle}/frame-00015.json'
        right = left.with_name(left.stem + '-right.json')
        a, b = json.loads(left.read_text()), json.loads(right.read_text())
        with Image.open(left.with_suffix('.png')) as image, Image.open(right.with_suffix('.png')) as other:
            difference = float(np.abs(np.array(image.convert('RGB'), dtype=np.int16) -
                                      np.array(other.convert('RGB'), dtype=np.int16)).mean())
            thumbnail = image.convert('RGB'); thumbnail.thumbnail((600, 665))
            x, y = index % 2 * 600, index // 2 * 700
            sheet.paste(thumbnail, (x + (600-thumbnail.width)//2, y+30))
            draw.text((x+12, y+10), angle.upper() + ' | amplified stereo', fill='white')
        angles.append({'angle': angle, 'left': str(left.with_suffix('.png')),
                       'right': str(right.with_suffix('.png')), 'sameXRFrame': a['xrFrame'] == b['xrFrame'],
                       'sameHeadSelectedLayers': a['billboards'] == b['billboards'],
                       'eyeSeparationWorld': float(np.linalg.norm(np.array(a['camera'])-b['camera'])),
                       'meanStereoPixelDifference': difference,
                       'leftSha256': hashlib.sha256(left.with_suffix('.png').read_bytes()).hexdigest(),
                       'rightSha256': hashlib.sha256(right.with_suffix('.png').read_bytes()).hexdigest()})
    sheet.save(root / 'xr-four-angles.png')
    result = {'runs': documents, 'canonical': scores, 'angles': angles, 'visualSampleCount': 2, 'performanceSampleCount': 1,
              'visualBinarySHA256': json.loads((root/'two-sample-launch.json').read_text())['binarySHA256'],
              'headTranslation': json.loads((root / 'head-translation.json').read_text()),
              'binaryHashes': {name: hashlib.sha256((RUNTIME / name).read_bytes()).hexdigest()
                               for name in ('soju-bw-scene', 'soju-bw-xr')},
              'acceptance': {'canonicalRegions': scores['regionalSSIMPassed'],
                             'alignment': scores['spriteAlignment']['passed'],
                             'stereoGPUP95': final['stereoGPU']['p95Ms'] <= 11.1,
                             'frameIntervalP95': final['frameIntervals']['p95Ms'] <= 11.1,
                             'liveStreamingResidency': False, 'cliffs': False,
                             'allBuildingContacts': False, 'physicalHeadset': False},
              'limitations': ['Metal batches are explicitly opt-in frozen sprite-scene replay; gameplay wrist panels and rigged models are not supported.',
                              'Directional-frame changes still synchronously load and rebuild; motion can stall.',
                              'Cache byte budgets do not cap resources pinned by active scene nodes and batches.',
                              'Terrain uses texture arrays; HD sprite frames still use material texture bindings.',
                              'Cliff stretch and complete building contact acceptance remain open.',
                              'Four-angle swapchain captures use 2x MSAA; final performance uses single-sample rendering.',
                              'Canonical scoring exercises the retained SceneKit route, not the experimental Metal route.']}
    (root / 'acceptance.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result['acceptance'], indent=2))


if __name__ == '__main__':
    report()
