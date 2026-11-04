#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
OUT="$ROOT/dist/testtitles"
CORE="$ROOT/tools/switch/core"
mkdir -p "$OUT/stress"
cargo build --offline --release --manifest-path "$CORE/Cargo.toml" --features hv --example test_title
cp "$CORE/target/release/examples/test_title" "$OUT/runner"
codesign --force --sign - --entitlements "$CORE/hypervisor.entitlements" "$OUT/runner"
: > "$OUT/stress/summary.txt"
for round in $(seq 1 100); do
 if "$OUT/runner" "$OUT/thread_stress.nro" "$OUT/home-thread_stress" > "$OUT/stress/$round.log" 2>&1; then
  printf 'PASS|run=%s|' "$round" | tee -a "$OUT/stress/summary.txt"
  rg '^TOTAL\|' "$OUT/stress/$round.log" | tee -a "$OUT/stress/summary.txt"
 else
  echo "FAIL|run=$round" | tee -a "$OUT/stress/summary.txt"
  tail -15 "$OUT/stress/$round.log"
  exit 1
 fi
done
