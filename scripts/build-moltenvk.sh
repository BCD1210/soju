#!/usr/bin/env bash
# Build the MoltenVK in the Steam support package (libMoltenVK.dylib): upstream v1.4.2 plus
# patches/moltenvk/*.patch, x86_64 only (the Steam runtime's Wine runs under Rosetta).
#   bash scripts/build-moltenvk.sh [out-dir]
# Needs Xcode. 1.4.2 converts the DrawIndex shader input that 1.4.1 refused. Soju's patch,
# zero-missing-vertex-attributes, lets a pipeline build when its vertex shader reads a location the
# vertex input state does not describe: Vulkan allows that, Metal refuses the pipeline. Such inputs
# get an attribute in a constant zero buffer. Detroit: Become Human needs both.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TAG=v1.4.2
COMMIT=db66022459ffb663aa2b50f6b018bc2e124f5edf
OUT="${1:-$ROOT/build/moltenvk}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
git clone -q --depth 1 --branch "$TAG" https://github.com/KhronosGroup/MoltenVK.git "$WORK/src"
[ "$(git -C "$WORK/src" rev-parse HEAD)" = "$COMMIT" ] || { echo "MoltenVK $TAG is not $COMMIT" >&2; exit 1; }
for p in "$ROOT"/patches/moltenvk/*.patch; do git -C "$WORK/src" apply "$p"; done
(cd "$WORK/src" && ./fetchDependencies --macos)
(cd "$WORK/src" && xcodebuild build -project MoltenVKPackaging.xcodeproj -scheme "MoltenVK Package (macOS only)" \
  -destination "generic/platform=macOS" -configuration Release ARCHS=x86_64 ONLY_ACTIVE_ARCH=NO -quiet)
DYLIB="$WORK/src/Package/Release/MoltenVK/dynamic/dylib/macOS/libMoltenVK.dylib"
strings -a "$DYLIB" > "$WORK/strings.txt"
grep -q "is not in the vertex input state; it reads zero" "$WORK/strings.txt" || { echo "patch missing from build" >&2; exit 1; }
mkdir -p "$OUT"
cp "$DYLIB" "$OUT/libMoltenVK.dylib"
cp "$WORK/src/LICENSE" "$OUT/MoltenVK-Apache.txt"
echo "Built $OUT/libMoltenVK.dylib ($(shasum -a 256 "$OUT/libMoltenVK.dylib" | cut -c1-64))"
