"""Summarize private M11 evidence without distributing game pixels or snapshots."""
import json
from pathlib import Path
from PIL import Image, ImageDraw
from client import RUNTIME


def report():
    root = RUNTIME / '3d-proof/m11'
    canonical = json.loads((root / 'canonical-final/frame-00020-scores.json').read_text())
    timing = json.loads((root / 'final-diagnostic-final-metrics.json').read_text())
    images = Image.new('RGB', (1600, 960), '#15171b')
    draw = ImageDraw.Draw(images)
    for index, angle in enumerate(('ne', 'nw', 'sw', 'se')):
        x, y = (index % 2) * 800, (index // 2) * 480
        draw.text((x+12, y+8), angle.upper()+' — frozen snapshot, left / right', fill='white')
        for eye, suffix in enumerate(('', '-right')):
            with Image.open(root / f'final-{angle}/frame-00025{suffix}.png') as image:
                image.thumbnail((390, 440))
                images.paste(image, (x+eye*400, y+30))
    images.save(root / 'xr-four-angles.png')
    lines = ['# M11 diagnostic timing', '',
             '34-object frozen snapshot, capture disabled, 30-second warm-up before arming.',
             '**Not live gameplay and not the requested ten-minute 1v1 acceptance run.**', '',
             '| Measurement | Count | Mean ms | p95 ms | p99 ms |',
             '| --- | ---: | ---: | ---: | ---: |']
    for label, key in [('Cadence', 'frameIntervals'), ('Stereo CPU submission', 'stereoCPUEncode'), ('Stereo GPU', 'stereoGPU')]:
        row = timing[key]
        lines.append(f"| {label} | {row['count']} | {row['meanMs']:.3f} | {row['p95Ms']:.3f} | {row['p99Ms']:.3f} |")
    lines += ['', f"Measured span: {timing['elapsedSeconds']:.3f} seconds.",
              f"Estimated missed refresh slots: {timing['frameIntervals']['estimatedMissedRefreshSlots']}.",
              'Misses are inferred from cadence, not measured compositor drops. Percentile resolution is 0.01 ms.',
              '', 'Live 1v1 / 600 seconds: **unmeasured**; the dedicated game loader did not open the bridge port.',
              'See `live-startup-blocker.json` and `game-launch.log`.']
    (root / 'timings.md').write_text('\n'.join(lines)+'\n')
    acceptance = {
        'defaultRenderer': 'Metal batches on desktop and XR; SceneKit overlay for other geometry',
        'fallback': 'SOJU_BW_RENDERER=scenekit',
        'canonical': canonical,
        'launchEvidence': {name: json.loads((root / file).read_text()) for name, file in [
            ('canonical', 'canonical-final-launch.json'),
            ('diagnostic', 'final-diagnostic-launch.json'),
            *[(angle, f'final-{angle}-launch.json') for angle in ('ne', 'nw', 'sw', 'se')],
        ]},
        'verification': {'pythonTests': 277, 'pythonSubtests': 60, 'rustTests': 14,
                         'desktopRuntimeChecks': 107, 'xrRuntimeChecks': 107, 'fallbackRuntimeChecks': 107},
        'canonicalReference': 'Independent HD compositor, not a Remastered framebuffer',
        'diagnosticTiming': timing,
        'liveTenMinuteAcceptance': False,
        'cliffStretchingResolved': False,
        'allBuildingTypesGroundedFromFourAngles': False,
        'zeroPerFrameAllocations': False,
        'hardTotalResidencyBudget': False,
        'completedFrameCacheBytes': 384*1024*1024,
        'residencyScope': 'Completed stream cache only; decoder caches and active draw references are additional',
        'overallAccepted': False,
    }
    (root / 'acceptance.json').write_text(json.dumps(acceptance, indent=2)+'\n')
    print(root / 'timings.md')
    print(root / 'acceptance.json')


if __name__ == '__main__':
    report()
