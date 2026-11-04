#!/usr/bin/env bash
# Build, ad-hoc sign and run only the synthetic Hypervisor integration tests.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
if [ "$(uname -s)" != Darwin ] || [ "$(uname -m)" != arm64 ]; then
  echo "Hypervisor smoke tests require Apple Silicon macOS." >&2
  exit 1
fi
CORE="$ROOT/tools/switch/core"
mkdir -p "$ROOT/dist"
MANIFEST=$(mktemp "$ROOT/dist/hv-tests.XXXXXX")
trap 'rm -f "$MANIFEST"' EXIT
cargo test --offline --manifest-path "$CORE/Cargo.toml" --features hv --test hv_smoke --no-run --message-format=json > "$MANIFEST"
BINARY=$(python3 - "$MANIFEST" <<'PY'
import json, sys
with open(sys.argv[1]) as source:
    paths = [item['executable'] for line in source if (item := json.loads(line)).get('reason') == 'compiler-artifact'
             and item.get('target', {}).get('name') == 'hv_smoke' and item.get('executable')]
if len(paths) != 1:
    raise SystemExit('Expected exactly one hv_smoke test executable')
print(paths[0])
PY
)
codesign --force --sign - --entitlements "$CORE/hypervisor.entitlements" "$BINARY"
codesign --verify --strict "$BINARY"
"$BINARY" --ignored --nocapture --test-threads=1
