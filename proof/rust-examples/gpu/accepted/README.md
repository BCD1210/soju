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
