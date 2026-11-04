#!/usr/bin/env bash
# Signed release acceptance and comparable headless framebuffer throughput.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
: "${SOJU_SWITCH_TEST_NRO:?Set SOJU_SWITCH_TEST_NRO to your local 2048 1.1.0 NRO}"
CORE="$ROOT/tools/switch/core"
mkdir -p "$ROOT/dist"
MANIFEST=$(mktemp "$ROOT/dist/homebrew-tests.XXXXXX")
BENCH_HOME=$(mktemp -d "$ROOT/dist/homebrew-bench.XXXXXX")
trap 'rm -f "$MANIFEST"; rm -rf "$BENCH_HOME"' EXIT
cargo test --offline --release --manifest-path "$CORE/Cargo.toml" --features hv --test homebrew --no-run --message-format=json > "$MANIFEST"
BINARY=$(python3 - "$MANIFEST" <<'PY'
import json, sys
with open(sys.argv[1]) as f:
    paths = [i['executable'] for line in f if (i := json.loads(line)).get('executable') and i.get('target', {}).get('name') == 'homebrew']
if len(paths) != 1:
    raise SystemExit('Expected one homebrew integration test')
print(paths[0])
PY
)
codesign --force --sign - --entitlements "$CORE/hypervisor.entitlements" "$BINARY"
codesign --verify --strict "$BINARY"
"$BINARY" --ignored --nocapture --test-threads=1
PYTHON="${SOJU_SWITCH_PYTHON:-$HOME/.local/share/soju/switch/venv/bin/python}"
# Reuse an installed frame converter if present; never modify the user's installation.
FRAME_LIB="${SOJU_SWITCH_HOME:-$HOME/.local/share/soju/switch}/libsoju-switch-frame.dylib"
if [ -f "$FRAME_LIB" ]; then cp "$FRAME_LIB" "$BENCH_HOME/"; fi
SOJU_SWITCH_HOME="$BENCH_HOME" "$PYTHON" "$ROOT/tools/switch/trace_nro.py" "$SOJU_SWITCH_TEST_NRO" --benchmark-frames 60 --max-steps 1000000000
