#!/usr/bin/env bash
set -euo pipefail
unset SOJU_SWITCH_KEY_DIR
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
    docked=0
    if [ "$name" = media ]; then docked=1; fi
    if SOJU_SWITCH_DOCKED="$docked" "$OUT/runner" "$probe" "$OUT/home-$name" > "$OUT/$name.log" 2>&1; then
        echo "PASS $name"
    else
        echo "FAIL $name"
        failed=1
    fi
    cat "$OUT/$name.log"
done
cargo run --offline --release --manifest-path "$CORE/Cargo.toml" --example package_title -- "$OUT"
mkdir -p "$OUT/home-retail/keys"
cp "$OUT/synthetic.keys" "$OUT/home-retail/keys/prod.keys"
chmod 600 "$OUT/home-retail/keys/prod.keys"
for round in 1 2; do
    if "$OUT/runner" "$OUT/storage.nsp" "$OUT/home-retail" > "$OUT/retail-$round.log" 2>&1; then
        echo "PASS retail-$round"
    else
        echo "FAIL retail-$round"
        failed=1
    fi
    cat "$OUT/retail-$round.log"
done
if ! rg -q 'PASS\|previous committed save' "$OUT/retail-2.log"; then failed=1; fi
exit "$failed"
