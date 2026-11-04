#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
CORE="$ROOT/tools/switch/core"
OUT="$ROOT/dist/parallel-benchmark"
mkdir -p "$OUT" "$OUT/home"
bash "$ROOT/tools/switch/testtitles/build.sh" > "$OUT/build.txt" 2>&1
cargo build --offline --release --manifest-path "$CORE/Cargo.toml" --features hv --example test_title
cp "$CORE/target/release/examples/test_title" "$OUT/runner"
codesign --force --sign - --entitlements "$CORE/hypervisor.entitlements" "$OUT/runner"
python3 - "$ROOT" "$OUT" <<'PY'
import json, os, re, statistics, subprocess, sys
from pathlib import Path
root, out = map(Path, sys.argv[1:])
rows = []
for run in range(1, 6):
    row = {'run': run}
    for mode in ('serial', 'parallel'):
        env = dict(os.environ)
        env.pop('SOJU_SERIALIZED', None)
        if mode == 'serial':
            env['SOJU_SERIALIZED'] = '1'
        result = subprocess.run([str(out/'runner'), str(root/'dist/testtitles/parallel_cpu.nro'), str(out/'home')], env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=30)
        (out/f'{mode}-{run}.txt').write_text(result.stdout)
        assert result.returncode == 0 and 'failures=0' in result.stdout and 'FAIL|' not in result.stdout
        ticks = re.search(r'COMPUTE\|ticks=(\d+)\|hz=19200000', result.stdout)
        profile = re.search(r'PROFILE\|elapsed_ms=([\d.]+)\|max_parallel=(\d+)\|timer_preemptions=(\d+)', result.stdout)
        assert ticks and profile
        assert int(profile[2]) == (1 if mode == 'serial' else 4)
        assert int(profile[3]) > 0
        row[mode+'_ms'] = int(ticks[1]) / 19200
    rows.append(row)
serial = statistics.median(row['serial_ms'] for row in rows)
parallel = statistics.median(row['parallel_ms'] for row in rows)
report = {'runs': rows, 'serial_median_ms': serial, 'parallel_median_ms': parallel, 'speedup': serial / parallel, 'target_met': serial / parallel >= 3}
(out/'summary.json').write_text(json.dumps(report, indent=2)+'\n')
print(json.dumps(report, indent=2))
PY
