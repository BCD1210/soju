#!/usr/bin/env bash
# Build the MoltenVK in the Steam support package (libMoltenVK.dylib): upstream v1.4.2 plus
# patches/moltenvk/*.patch, x86_64 only (the Steam runtime's Wine runs under Rosetta).
#   bash scripts/build-moltenvk.sh [out-dir]
# Needs Xcode. The patches come from Highball (GPL-3.0-or-later): cache-drawid-present fixes the
# draw ID of direct draws, an empty pipeline cache read and a present that read a released image;
# lenient-interfaces (only with HB_MVK_LENIENT_INTERFACES=1) gives a vertex attribute the pipeline
# does not describe a placeholder and lets a color attachment take the fragment output's kind.
# Together they let Detroit: Become Human compile its pipelines (DrawIndex) and draw.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TAG=v1.4.2
COMMIT=db66022459ffb663aa2b50f6b018bc2e124f5edf
OUT="${1:-$ROOT/build/moltenvk}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
git clone -q --depth 1 --branch "$TAG" https://github.com/KhronosGroup/MoltenVK.git "$WORK/src"
[ "$(git -C "$WORK/src" rev-parse HEAD)" = "$COMMIT" ] || { echo "MoltenVK $TAG is not $COMMIT" >&2; exit 1; }
for p in cache-drawid-present lenient-interfaces; do git -C "$WORK/src" apply "$ROOT/patches/moltenvk/$p.patch"; done
(cd "$WORK/src" && ./fetchDependencies --macos)
(cd "$WORK/src" && xcodebuild build -project MoltenVKPackaging.xcodeproj -scheme "MoltenVK Package (macOS only)" \
  -destination "generic/platform=macOS" -configuration Release ARCHS=x86_64 ONLY_ACTIVE_ARCH=NO -quiet)
DYLIB="$WORK/src/Package/Release/MoltenVK/dynamic/dylib/macOS/libMoltenVK.dylib"
MARKS="$(strings -a "$DYLIB")"
for p in cache-drawid-present lenient-interfaces; do
  grep -q "HIGHBALL $p" <<< "$MARKS" || { echo "patch missing from build: $p" >&2; exit 1; }
done
mkdir -p "$OUT"
cp "$DYLIB" "$OUT/libMoltenVK.dylib"
cp "$WORK/src/LICENSE" "$OUT/MoltenVK-Apache.txt"
echo "Built $OUT/libMoltenVK.dylib ($(shasum -a 256 "$OUT/libMoltenVK.dylib" | cut -c1-64))"
