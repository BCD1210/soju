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
