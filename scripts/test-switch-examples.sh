#!/usr/bin/env bash
# Run locally supplied devkitPro examples without a window or host input.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CORE="$ROOT/tools/switch/core"
CORPUS="${SOJU_SWITCH_EXAMPLES:-$HOME/.local/share/soju/switch/examples}"
PROOF="${SOJU_SWITCH_PROOF:-$HOME/.local/share/soju/switch/proof/rust-examples}"
cargo build --offline --release --manifest-path "$CORE/Cargo.toml" --features hv
mkdir -p "$ROOT/dist"
RUN=$(mktemp -d "$ROOT/dist/examples-run.XXXXXX")
trap 'rm -rf "$RUN"' EXIT
BINARY="$RUN/soju-switch-core"
cp "$CORE/target/release/soju-switch-core" "$BINARY"
codesign --force --sign - --entitlements "$CORE/hypervisor.entitlements" "$BINARY"
codesign --verify --strict "$BINARY"
python3 - "$BINARY" "$CORPUS" "$PROOF" <<'PY'
import os
from pathlib import Path
import subprocess
import shutil
import sys
import tempfile

binary, corpus, proof = map(Path, sys.argv[1:])
examples = sorted(corpus.glob('*.nro'))
if not examples:
    raise SystemExit(f'No examples in {corpus}')
proof.mkdir(parents=True, exist_ok=True)
timeout = float(os.environ.get('SOJU_SWITCH_TIMEOUT', '15'))
frames = os.environ.get('SOJU_SWITCH_FRAMES', '1')
font = Path(os.environ.get('SOJU_SWITCH_FONT', str(Path.home() / '.local/share/soju/switch/host-unicode-bmp.ttf')))
with (proof / 'summary.txt').open('w') as summary:
    summary.write('name|result|detail\n')
    for nro in examples:
        capture = proof / f'{nro.stem}.png'
        capture.unlink(missing_ok=True)
        with tempfile.TemporaryDirectory(prefix='soju-example-') as home:
            if font.is_file():
                shutil.copyfile(font, Path(home) / 'host-unicode-bmp.ttf')
            env = dict(os.environ, SOJU_SWITCH_HOME=str(Path(home).resolve()))
            with (proof / f'{nro.stem}.log').open('w+') as log:
                detail = ''
                try:
                    result = subprocess.run(
                        [str(binary), '--headless', str(nro), '--frames', frames,
                         '--capture', str(capture)], env=env, stdout=log,
                        stderr=subprocess.STDOUT, timeout=timeout, check=False)
                    detail = f'process status {result.returncode}'
                except subprocess.TimeoutExpired:
                    detail = f'timeout after {timeout:g}s'
                log.seek(0)
                lines = log.read().splitlines()
        errors = [line for line in lines if line.startswith('soju-switch-core:')]
        if errors:
            detail = errors[0]
        status = 'stop'
        if any(line.startswith('exit0|') for line in lines):
            status = 'exit0'
        if capture.is_file() and any(line.startswith('frame|') for line in lines):
            status = 'frame'
        row = f'{nro.stem}|{status}|{detail.replace(chr(124), ";")}\n'
        summary.write(row)
        summary.flush()
        print(row, end='', flush=True)
PY
