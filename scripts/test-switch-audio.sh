#!/usr/bin/env bash
# Generates a synthetic tone NRO and checks real AudioUnit callback consumption.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CORE="$ROOT/tools/switch/core"
mkdir -p "$ROOT/dist"
MANIFEST=$(mktemp "$ROOT/dist/audio-tests.XXXXXX")
trap 'rm -f "$MANIFEST"' EXIT
cargo test --offline --release --manifest-path "$CORE/Cargo.toml" --features desktop --test audio_host --no-run --message-format=json > "$MANIFEST"
BINARY=$(python3 - "$MANIFEST" <<'PY'
import json, sys
with open(sys.argv[1]) as f:
    paths = [i['executable'] for line in f if (i := json.loads(line)).get('executable') and i.get('target', {}).get('name') == 'audio_host']
if len(paths) != 1:
    raise SystemExit('Expected one audio integration test')
print(paths[0])
PY
)
codesign --force --sign - --entitlements "$CORE/hypervisor.entitlements" "$BINARY"
codesign --verify --strict "$BINARY"
"$BINARY" --ignored --nocapture --test-threads=1
