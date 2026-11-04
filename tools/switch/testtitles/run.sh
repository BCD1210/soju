#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
OUT="$ROOT/dist/testtitles"
CORE="$ROOT/tools/switch/core"
bash "$ROOT/tools/switch/testtitles/build.sh"
cargo build --offline --release --manifest-path "$CORE/Cargo.toml" --features hv --example test_title
cp "$CORE/target/release/examples/test_title" "$OUT/runner"
codesign --force --sign - --entitlements "$CORE/hypervisor.entitlements" "$OUT/runner"
failed=0
for probe in "$OUT"/*.nro; do
    name="$(basename "$probe" .nro)"
    mkdir -p "$OUT/home-$name"
    font="$HOME/.local/share/soju/switch/host-unicode-bmp.ttf"
    if [ -f "$font" ]; then cp "$font" "$OUT/home-$name/host-unicode-bmp.ttf"; fi
    if "$OUT/runner" "$probe" "$OUT/home-$name" > "$OUT/$name.log" 2>&1; then
        echo "PASS $name"
    else
        echo "FAIL $name"
        failed=1
    fi
    cat "$OUT/$name.log"
done
exit "$failed"
