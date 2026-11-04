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
`--warmup-ms 1000` excludes menu/startup frames from the frame count while inputs
still advance from process start. A is pressed at 100 ms. No window is opened.
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
is four to allow the small initial animation time. Outside pixels are black.
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

- Capture SHA-256: `0b60192ab10cf45532e905d6d4adc8f9d4e12f946df7a8c04716b1eacf889702`

## deko3d Example04: Textured Cube

`deko-example04.png` is the first-pose native Metal capture. The headless test
checks 851,810 pixels using an independent BC1 decoder and bilinear sampling of
the locally installed `cat-256x256.bc1`. It checks the projected front face and
black background; the projection edge and high-gradient texel transitions are
excluded to accommodate initial animation time. Channel tolerance is six and
more than 30,000 interior texture pixels must be checked. The source asset is
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

- Capture SHA-256: `f74115efb0e22f5b6c01fdb7f9ce4c8fb28920dd61f912663ad1d517adb2a563`

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

Run all five scenes with `bash scripts/test-switch-gpu-accepted.sh`.
Native scenes run serially: concurrent compilation stalls can advance the
animated cubes beyond the initial-pose oracle's tolerances. These tests remain
initial-pose checks; they do not establish animation timing correctness.

- Capture SHA-256: `ed71b59d80bec48168cc97ca8abb3d270799b6e94b4ae6a5b7e1ef08d72c8033`
- Original NRO SHA-256: `b693640bd253745ca224e406d71328213339bee8a968dcdff752df32e6f61745`
- Original `main.cpp` SHA-256: `7220ec259c1710d753567b861135c4d630087bfee9a79af575744b8cf98be885`
