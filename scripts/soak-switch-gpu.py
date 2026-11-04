#!/usr/bin/env python3
"""Run accepted scenes ten times with four sustained CPU workers."""
import hashlib
import multiprocessing
import os
import json
import shutil
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / 'dist/gpu-soak'


def load(stop, result):
    block = bytes(1024 * 1024)
    start = time.process_time()
    iterations = 0
    while not stop.is_set():
        hashlib.sha256(block).digest()
        iterations += 1
    result.put((os.getpid(), iterations, time.process_time() - start))


if __name__ == '__main__':
    OUT.mkdir(parents=True, exist_ok=True)
    subprocess.run(['bash', str(ROOT / 'scripts/test-switch-gpu-accepted.sh')], cwd=ROOT, check=True)
    manifest = subprocess.check_output(['cargo','test','--offline','--manifest-path',str(ROOT/'tools/switch/core/Cargo.toml'),'--features','desktop','--test','gpu_examples','--no-run','--message-format=json'], cwd=ROOT, text=True)
    binaries=[v['executable'] for line in manifest.splitlines() if (v:=json.loads(line)).get('executable') and v.get('target',{}).get('name')=='gpu_examples']
    if len(binaries)!=1:
        raise SystemExit('Expected one GPU test executable')
    runner=OUT/'runner'
    test=OUT/'gpu_examples'
    shutil.copy2(ROOT/'dist/gpu-accepted/runner',runner)
    shutil.copy2(binaries[0],test)
    env=dict(os.environ,SOJU_SWITCH_GPU_TEST_RUNNER=str(runner),
        SOJU_SWITCH_GPU_TEST_NRO=str(Path.home()/'.local/share/soju/switch/examples/graphics__deko3d__deko_examples.nro'),
        SOJU_SWITCH_GPU_TEST_GL_NRO=str(Path.home()/'.local/share/soju/switch/examples/graphics__opengl__simple_triangle.nro'),
        SOJU_SWITCH_GPU_TEST_BASIC_NRO=str(Path.home()/'.local/share/soju/switch/examples/graphics__deko3d__deko_basic.nro'),
        SOJU_SWITCH_GPU_TEST_BC1=str(Path.home()/'devkitpro/opt/devkitpro/examples/switch/graphics/deko3d/deko_examples/romfs/cat-256x256.bc1'))
    (OUT/'binaries.sha256').write_text(''.join(f'{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n' for p in (runner,test)))
    stop = multiprocessing.Event()
    result = multiprocessing.Queue()
    workers = [multiprocessing.Process(target=load, args=(stop, result)) for _ in range(4)]
    for worker in workers:
        worker.start()
    try:
        with (OUT / 'results.csv').open('w') as report:
            report.write('run,status,seconds,example04_sha256\n')
            expected_digest = None
            for run in range(1, 11):
                output = OUT / str(run)
                output.mkdir(exist_ok=True)
                start = time.monotonic()
                with (output / 'tests.log').open('w') as log:
                    status = subprocess.run([str(test), '--ignored', '--nocapture', '--test-threads=1'],
                        cwd=ROOT, env=dict(env, SOJU_SWITCH_GPU_TEST_OUTPUT=str(output)),
                        stdout=log, stderr=subprocess.STDOUT, timeout=600).returncode
                capture = output / 'deko-example04.png'
                digest = hashlib.sha256(capture.read_bytes()).hexdigest() if capture.exists() else ''
                if status == 0 and (not digest or expected_digest not in (None, digest)):
                    print('Example04 capture bytes changed', flush=True)
                    status = 1
                expected_digest = digest
                row = f'{run},{status},{time.monotonic()-start:.3f},{digest}\n'
                report.write(row)
                report.flush()
                print(row, end='', flush=True)
                if status:
                    raise SystemExit(status)
    finally:
        stop.set()
        for worker in workers:
            worker.join()
        with (OUT / 'load.csv').open('w') as report:
            report.write('pid,sha256_megabytes,cpu_seconds\n')
            for _ in workers:
                pid, iterations, seconds = result.get()
                report.write(f'{pid},{iterations},{seconds:.3f}\n')
