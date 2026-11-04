#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CORE="$ROOT/tools/switch/core"
mkdir -p "$ROOT/dist/gpu-accepted"
cargo build --offline --release --manifest-path "$CORE/Cargo.toml" --features desktop
cp "$CORE/target/release/soju-switch-core" "$ROOT/dist/gpu-accepted/runner"
codesign --force --sign - --entitlements "$CORE/hypervisor.entitlements" "$ROOT/dist/gpu-accepted/runner"
codesign --verify --strict "$ROOT/dist/gpu-accepted/runner"
export SOJU_SWITCH_GPU_TEST_RUNNER="$ROOT/dist/gpu-accepted/runner"
export SOJU_SWITCH_GPU_TEST_NRO="${SOJU_SWITCH_GPU_TEST_NRO:-$HOME/.local/share/soju/switch/examples/graphics__deko3d__deko_examples.nro}"
export SOJU_SWITCH_GPU_TEST_BASIC_NRO="${SOJU_SWITCH_GPU_TEST_BASIC_NRO:-$HOME/.local/share/soju/switch/examples/graphics__deko3d__deko_basic.nro}"
export SOJU_SWITCH_GPU_TEST_GL_NRO="${SOJU_SWITCH_GPU_TEST_GL_NRO:-$HOME/.local/share/soju/switch/examples/graphics__opengl__simple_triangle.nro}"
export SOJU_SWITCH_GPU_TEST_GEARS_NRO="${SOJU_SWITCH_GPU_TEST_GEARS_NRO:-$HOME/.local/share/soju/switch/examples/graphics__opengl__es2gears.nro}"
export SOJU_SWITCH_GPU_TEST_OUTPUT="${SOJU_SWITCH_GPU_TEST_OUTPUT:-$ROOT/dist/gpu-accepted}"
export SOJU_SWITCH_GPU_TEST_BC1="${SOJU_SWITCH_GPU_TEST_BC1:-$HOME/devkitpro/opt/devkitpro/examples/switch/graphics/deko3d/deko_examples/romfs/cat-256x256.bc1}"
cargo test --offline --manifest-path "$CORE/Cargo.toml" --features desktop --test gpu_examples --test gpu_gears -- --ignored --nocapture --test-threads=1
