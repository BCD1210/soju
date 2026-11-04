#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
CORE="$ROOT/tools/switch/core"
OUT="${SOJU_CONTENTION_OUTPUT:-$ROOT/dist/contention-benchmark}"
mkdir -p "$OUT" "$OUT/home"
cargo build --offline --release --manifest-path "$CORE/Cargo.toml" --features hv --example test_title
cp "$CORE/target/release/examples/test_title" "$OUT/runner"
codesign --force --sign - --entitlements "$CORE/hypervisor.entitlements" "$OUT/runner"
python3 - "$ROOT" "$OUT" <<'PY'
import json, os, re, statistics, subprocess, sys
from pathlib import Path
root, out = map(Path, sys.argv[1:])
runs = int(os.environ.get('SOJU_CONTENTION_RUNS', '100'))
assert runs > 0
rows = []
for run in range(runs):
    row = {'run': run + 1}
    for mode in (('serial', 'parallel') if run % 2 == 0 else ('parallel', 'serial')):
        env = dict(os.environ)
        env.pop('SOJU_PROFILE', None)
        env.pop('SOJU_SERIALIZED', None)
        if mode == 'serial':
            env['SOJU_SERIALIZED'] = '1'
        result = subprocess.run([str(out/'runner'), str(root/'dist/testtitles/thread_stress.nro'), str((out/'home').resolve())], env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=30)
        (out/f'{mode}-{run + 1}.txt').write_text(result.stdout)
        assert result.returncode == 0 and 'failures=0' in result.stdout and 'FAIL|' not in result.stdout
        elapsed = re.search(r'PROFILE\|elapsed_ms=([\d.]+)', result.stdout)
        assert elapsed
        row[mode + '_ms'] = float(elapsed[1])
    rows.append(row)
    if (run + 1) % 20 == 0:
        print(f'paired runs: {run + 1}', flush=True)
serial = statistics.median(row['serial_ms'] for row in rows)
parallel = statistics.median(row['parallel_ms'] for row in rows)
report = {'runs': rows, 'serial_median_ms': serial, 'parallel_median_ms': parallel, 'ratio': serial / parallel, 'target_met': parallel <= serial}
(out/'summary.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({k: v for k, v in report.items() if k != 'runs'}, indent=2))
PY
