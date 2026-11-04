#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
OUT="$ROOT/dist/testtitles/dynamic"
CORE="$ROOT/tools/switch/core"
bash "$ROOT/tools/switch/testtitles/dynamic/build.sh"
cargo build --offline --release --manifest-path "$CORE/Cargo.toml" --features hv --example dynamic_title
cp "$CORE/target/release/examples/dynamic_title" "$OUT/runner"
codesign --force --sign - --entitlements "$CORE/hypervisor.entitlements" "$OUT/runner"
"$OUT/runner" "$OUT" | tee "$OUT/run.log"
