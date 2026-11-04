#!/usr/bin/env python3
"""Record headless GPU method outcomes; a captured menu is never acceptance."""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('binary', type=Path, help='isolated, Hypervisor-entitled runner')
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--corpus', type=Path, default=Path.home() / '.local/share/soju/switch/examples')
parser.add_argument("--scene", help="probe one named scene")
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
scenes = [(f'deko-example{i:02}', 'graphics__deko3d__deko_examples', i - 1) for i in range(1, 5)]
scenes += [('deko-basic', 'graphics__deko3d__deko_basic', None)]
scenes += [(f'opengl-{name}', f'graphics__opengl__{name}', None) for name in ('simple_triangle', 'textured_cube', 'es2gears')]
if args.scene:
    scenes = [scene for scene in scenes if scene[0] == args.scene]
    if not scenes:
        parser.error("unknown scene")
with (args.output / 'summary.txt').open('w') as summary:
    summary.write('scene|result|detail\n')
    for scene, name, selection in scenes:
        with tempfile.TemporaryDirectory(prefix='gpu-probe-', dir=args.output.resolve()) as home:
            argv = [str(args.binary.resolve()), '--headless', str((args.corpus / (name + '.nro')).resolve()), '--frames', '100000', '--capture', str(Path(home) / 'unaccepted.png')]
            if selection is not None:
                import json
                states = []
                for i in range(selection):
                    states += [{'at_ms': 100 + i * 200, 'buttons': 32768}, {'at_ms': 200 + i * 200, 'buttons': 0}]
                states += [{'at_ms': 100 + selection * 200, 'buttons': 1}]
                inputs = Path(home) / 'inputs.json'
                inputs.write_text(json.dumps(states))
                argv += ['--inputs', str(inputs)]
            with (args.output / (scene + '.log')).open('w+') as log:
                try:
                    result = subprocess.run(argv, env=dict(os.environ, SOJU_SWITCH_HOME=home, SOJU_SWITCH_GPU='metal', SOJU_SWITCH_TRACE_GPU='1'), stdout=log, stderr=subprocess.STDOUT, timeout=30, check=False)
                    detail = f'process status {result.returncode}; no acceptance check'
                except subprocess.TimeoutExpired:
                    detail = 'timeout; no acceptance check'
                log.seek(0)
                lines = log.read().splitlines()
            errors = [line for line in lines if line.startswith('soju-switch-core:')]
            if errors:
                detail = errors[-1].replace('|', ';')
            # Keep every dispatched method in order, including macro-generated writes.
            (args.output / (scene + '.methods')).write_text('\n'.join(line for line in lines if line.startswith('gpu-status|')) + '\n')
            row = f'{scene}|not-accepted|{detail}\n'
            summary.write(row)
            summary.flush()
            print(row, end='', flush=True)
