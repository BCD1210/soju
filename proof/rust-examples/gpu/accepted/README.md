# Accepted GPU scenes

## deko3d Example01: Simple Setup

`deko-example01.png` is a 1280×720 native Metal capture from the installed
unmodified devkitPro example. Source: `Example01_SimpleSetup.cpp`.
The source draws scissored clears, not a triangle (the triangle is Example02).
All 921,600 pixels pass the source-derived rectangle/color oracle with a maximum
one-unit channel tolerance. The test requires a dispatched Metal clear and a
successful runner exit, so a software menu cannot pass.

Reproduce: `bash scripts/test-switch-gpu-accepted.sh`.
The script builds/signs an isolated runner; it never replaces the installed one.
`--capture-tick 0` fixes both architectural and SVC tick reads at zero.
Scripted input transitions cross presentation boundaries and retain their spacing
after host stalls. The first presentation after the final input is captured,
without wall-time warmup. No window is opened. Host scheduling deadlines still
advance; this is an explicit pose-capture mode, not normal playback timing.
The test writes fresh captures/logs to `dist/gpu-accepted/`.
Ordered outcomes and policy counters are in `deko-example01.methods`.

- `graphics__deko3d__deko_examples.nro` SHA-256: `dcff7d2bd9163721dbdcfd0f090b88b98e73d8836b38178b75cd1d24de31f6e7`
- `Example01_SimpleSetup.cpp` SHA-256: `61d738c0ad3d40028d1833c41133f2e28d1a18393a8c22b51b5d6d3c1e9ab309`
- `deko-example01.png` SHA-256: `ec489a583c7672ff354e06a1b0698597854a7f00c54335b652dcd5507854c801`

## deko3d Example02: Triangle

`deko-example02.png` is the native 1280×720 capture from the same unmodified NRO.
The headless test checks 912,978 pixels against barycentric RGB interpolation of
the source vertices, including transparent black outside the triangle. A narrow
edge band is excluded; channel tolerance is two. The test requires the three-vertex
Metal draw to complete. Red is at the top, green bottom-left, blue bottom-right.

The bridge bounds straight-line shader discovery by the containing GPU mapping
and a 1 MiB limit, ending only at an unconditional EXIT. Branch/reconvergence
programs are rejected instead of scanning neighboring data. Both stages are
translated through soju-switch-shader. SPH export maps control active clip
distances; point-size state does not affect triangles. The guest Y viewport
scale is converted to the backend's Metal coordinate convention.

Fragment IPA multiply restores the pre-perspective numerator from Metal's
interpolated input and reciprocal W, then applies the guest register multiplier.
The installed homebrew's PASS-W / reciprocal / IPA sequence establishes this
bounded profile; a native synthetic W=2 test checks the arithmetic separately.
Centroid, offset, indexed and sample-predicate forms remain rejected.

Reproduce both accepted scenes: `bash scripts/test-switch-gpu-accepted.sh`.

- Capture SHA-256: `2f391c3ab3f8edcde6db39d95d5b6bea7070fa53a4b6a125b3b8075c7b030a65`

## deko3d Example03: Cube

`deko-example03.png` captures the initial cube pose. The source uses six quads,
model/view and projection uniform matrices, D24/stencil, and ZCULL setup.
The pixel test checks 912,096 pixels against the source's initial front-face
projection (40-degree FOV, half-unit scale, camera translation -3) and the
quad's interpolated colors. Edge samples are excluded and channel tolerance
is four. Guest animation elapsed time is exactly zero. Outside pixels are black.
This is first-pose acceptance, not a rotating/multiple-frame depth oracle.
Native regressions independently verify depth comparisons/storage, nonzero
single-layer depth pitch, and the quad's two-triangle coverage.

Nonindexed, complete smooth-shaded quads are expanded into triangles in host
index storage; indexed/flat-shaded quads remain rejected. ZCULL clear, invalidate,
sync and metadata setup are logged no-ops, while actual depth/stencil clears
and tests execute through Metal. Store/load serialization is not implemented.
The source's discard-depth hint preserves completed storage; deko3d identifies
byte method `0xf78` as `DiscardRenderTarget`, so it is logged as a no-op.
Pixel shader barriers complete synchronously.

- Capture SHA-256: `22128714a728344fe409f51dbc7985df70015dc07d95cfcb01aa7d08dcbebbbf`

## deko3d Example04: Textured Cube

`deko-example04.png` is the first-pose native Metal capture. The headless test
checks 912,096 pixels using an independent BC1 decoder and bilinear sampling of
the locally installed `cat-256x256.bc1`. It checks the projected front face and
black background; only the projection edge is excluded. Sharp texture transitions
are now included. Channel tolerance remains six. The source asset is
not copied into the repository. Set `SOJU_SWITCH_GPU_TEST_BC1` to override it.

B0B5 copies support bounded pitched/block-linear line and rectangular transfers,
source/destination origins, identity component remaps, and overlap-safe capture.
All ranges are checked before writing. Block layout uses soju-switch-texture.
The scene also exercises B197 inline descriptor uploads, normalized bound TEXS
handles read from the selected guest constant buffer, split RGBA destinations,
BC1 sampling, independent sampler descriptors, uniforms and depth testing.

The runner initializes its GM20B context with Maxwell texture headers selected.
Descriptor support includes the observed single-mip LOD quality, sector promotion,
sampler-border and non-anisotropic defaults; comparison/reduction/anisotropic
filtering, additional mips and arbitrary descriptor modes remain unsupported.
B197/B1C0/A140 pitched inline uploads accept flush-only and sysmembar-disable
flags because the bridge publishes in order. Semaphore-release modes still stop.

- Capture SHA-256: `7eb0efa702f3c94b2b92097636fef18b0c1d8225b5620abcca90bacb3575def4`

## OpenGL simple_triangle

`opengl-simple_triangle.png` captures the original local devkitPro example,
using the same translated vertex/fragment path as deko. The test checks 918,400
pixels against the source triangle at (-.5,-.5), (+.5,-.5), (0,+.5), RGB vertex
colors and (.2,.3,.3,1) background. A narrow edge band is excluded; channel
tolerance is two. It requires a completed three-vertex draw and successful exit.

A140 uploads accept `0x1001`: synchronous launch with semaphore structure-size
selection but no semaphore release. Release modes remain rejected. The bridge
intersects viewport clip and scissor, validates unused higher-viewport defaults
(shader viewport-index output remains unsupported), and omits unused depth
attachments for color clears and draws with depth/stencil tests disabled.
Enabled depth bounds, nonzero depth layers and unsupported raster modes stop.

Run all six scenes with `bash scripts/test-switch-gpu-accepted.sh`.

## OpenGL es2gears

The suite now runs **seven scenes**. `opengl-es2gears.png` is the first native
Metal presentation of the unmodified local sample, with a fixed capture clock
and synchronous pipeline compilation. The probe's 30-second timeout is not used
as acceptance; `gpu_gears.rs` requires successful process completion and checks
the captured image.

The oracle independently intersects camera rays with the three gear front
planes using the source's 60-degree projection, view rotations (20,30,0), gear
centers, radii and widths. Samples inside the front annuli avoid teeth and edges;
front colors use the source materials and transformed normal dotted with the
normalized (5,5,10) light. Rays missing every bounded gear cylinder check the
black clear color. It checks **33,755 red + 3,860 green + 1,985 blue + 823,418
background = 863,018 RGB pixels**, tolerance two channel units. Side faces,
tooth edges and alpha are not validated by this oracle.

Required fixes: S8Z24 depth import/writeback, bounded control-flow shader extent
discovery, FMNMX.FTZ, and separate host bindings for vertex streams and constants.
The native depth and vertex-binding regressions failed before their fixes.
The control-flow tests also failed under a target-bound mutation and passed
after restoring the implementation. The FMNMX test failed before lowering.

- Capture SHA-256: `e18f109428a3b7bfe621184f8e466c003917d8fb85f09b901ddd0489f143176e`
- Local `es2gears/source/main.c`: `7a0208e4776261081c78c98dd4d3b8e8b3b0aa4de3cf690bfbda48a8e3fe47af`
- Local NRO: `e5f1f9140bd86ffe8fa77feaf421d93c6b89c40d5292680675672636b8ab2952`

Run `bash scripts/test-switch-gpu-accepted.sh`. No input NRO is committed.
Native scenes run serially. Host load cannot advance the frozen guest animation
counter. These tests remain initial-pose checks; they do not establish normal
animation timing correctness. `python3 scripts/soak-switch-gpu.py` copies immutable
runner/test binaries and runs ten iterations with four sustained CPU workers.

- Capture SHA-256: `ed71b59d80bec48168cc97ca8abb3d270799b6e94b4ae6a5b7e1ef08d72c8033`
- Original NRO SHA-256: `b693640bd253745ca224e406d71328213339bee8a968dcdff752df32e6f61745`
- Original `main.cpp` SHA-256: `7220ec259c1710d753567b861135c4d630087bfee9a79af575744b8cf98be885`

## Capture timing correction

`CApplication::run` records `tick_ref` before framebuffer setup; its first
`onFrame` receives the tick delta after setup. Example04 rotates from this delta.
The former host-clock counter therefore moved the first pose under load, even
when the first GPU frame was captured. Wall-time warmup could also select a later
frame. The capture harness now removes both timing dependencies. No sampling
precision adjustment or tolerance increase was needed; sharp texel checks now
pass as well. Normal desktop clocks and scheduling remain unchanged.

## deko_basic: vertex-ID triangle

`deko-basic.png` checks 912,978 pixels from the original 1280×720 sample:
source vertex positions and RGB barycentric interpolation, with the source
(.125, .294, .478, 1) background. Edge exclusion and channel tolerance two
match Example02. It requires a completed native three-vertex draw.

SPH input 0x2fc and ALD use Metal's integer vertex ID. Indexed scalar U32 LDC
uses the submitted constant-buffer byte length, a wide address calculation,
and alignment/range guards; invalid accesses return zero without host reads.
Lengths participate in shader cache identity. A later shorter binding is rejected.
Other LDC widths and addressing modes remain rejected.

- `deko-basic.png` SHA-256: `5809f3f22178cd7dc570c12e24d516fd5ac6454a58deee029ca2789bebe8aeb7`
- `main.c` SHA-256: `8f2a341b7f5f64e16a5cda941c5f6ad7615f0abf865683a1863bab63d3d44cf1`
- `triangle_vsh.glsl` SHA-256: `fc443ee5de622649102a3e9656bda4677f0500ac2e30689061f3cae9b297dbbe`
- `graphics__deko3d__deko_basic.nro` SHA-256: `79cfe7d71b644cc257dc2a23c488bca92984979fc5e0e3ca42c39b143b646e7f`

Run all six scenes with `bash scripts/test-switch-gpu-accepted.sh`.


SDL2 sdl2-simple is accepted with 2,073,600 exact RGBA checks on its first
1920x1080 presentation. The source initializes x=0, draws a gray (111) background,
and draws six 64x64 rectangles (RGB at the top and bottom), then increments x
after presenting. `gpu_sdl.rs` checks every pixel without edge exclusions.
The runner retains libnx NvKind_Z16 (1) and NvKind_Z16_2C (2); the existing Metal
D16 path provides canonical uncompressed guest storage, as with the other
compression kinds. Capture: `sdl2-simple.png`. The full-loop probe times out;
first-presentation acceptance is the result reported here.

## OpenGL lenny and dynamic_resolution

Two further scenes pass `gpu_lenny.rs` using the locally installed lenny mesh
(3,345 vertices, 1,115 triangles), source projection, per-instance transforms,
perspective-correct quaternion normals and view vectors, depth, and the source
ambient/diffuse/specular equation. No expected values come from a capture.
RGB tolerance is two; triangle interiors exclude 1.5-pixel edges, silhouettes
and boundaries between instances have a two-pixel guard. Alpha and animation
are outside these first-pose checks.

- lenny: 5,720 surface and 881,588 background pixels, zero mismatches.
- dynamic_resolution: 74,707 surface pixels across 41 visible instances and
  1,652,199 background pixels, zero mismatches. The source issues 64 instances;
  some are occluded or offscreen. Its initial handheld viewport is 1280x720
  within the 1920x1080 buffer.

Fixes include RRO/MUFU pairs, structured shader branches, FMUL.FMZ, MME
bitfield operations, integer instance IDs, and initial stage constant groups.
The group defaults model the initial context required by the observed stream:
fragment group four is used before an explicit binding-group write. NVIDIA's
public B197 header documents the fields, not these initial values. Explicit
writes override the modelled defaults; native tests check both paths.

Capture SHA-256:

- `opengl-lenny.png`: `9699464924dd0be990e6a04be2281196ac50d9a4c230d2387afe8564d1abfaa4`
- `opengl-dynamic_resolution.png`: `20a3d030daaadc17fd403f1864d9fbde0169b210bc7c405f4be3f26486df947a`

Run all ten scenes with `bash scripts/test-switch-gpu-accepted.sh`. The two
new mesh tests require `SOJU_SWITCH_GPU_TEST_LENNY_MESH`; the script defaults
to the local devkitPro source location, without copying the mesh into git.
