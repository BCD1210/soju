# Runner regression results

Branch: `feat/switch-core`. The requested shader tip `5e298df` was merged first
in `b6d6823`. All commits remain local.

## Graphics follow-up from 3deb0fb

One additional scene is accepted: **OpenGL es2gears**, with its capture under
`../gpu/accepted/` and 863,018 source-derived RGB pixel checks. The seven-scene
acceptance suite passes. This is partial progress on the requested graphics
expansion, not a completion or commercial-game compatibility claim.

- `60e8aeb`: S8Z24 guest depth/stencil import and writeback through Metal.
- `075c1db`: bounded control-flow shader discovery; forward targets beyond early
  exits are included and the resulting control flow is validated.
- `dee7fd7`: FMNMX.FTZ lowering, including 32 native/reference subnormal cases.
- `fc4d8d9`: vertex streams receive host buffer slots independent of shader cbufs.
- `b2dba5e`: read-only embedded DKSH audit with no extracted shader files.
- `3f8f27b`: accepted es2gears capture and projected geometry/lighting checks.

Evidence: [`graphics-followup/`](graphics-followup/). Validation totals:
core library **109 passed**; shader **46 passed**; GPU **36 passed, 5 existing
ignored benchmarks/soaks**; accepted scenes **7 passed**. Core (desktop), shader
and GPU all-target Clippy checks pass with warnings denied. Depth and binding
tests were observed failing before implementation; control-flow target mutation
failed both new extent tests; restoring the implementation passed all 109 core
tests. The first gears oracle run exposed an empty blue-gear sampling interval;
the analytic inset was corrected before accepting it, without changing RGB
tolerance or using image-derived expected values.

The final 59-NRO rerun uses the same default backend and one-frame budget as the
baseline: **34 frames / 24 stops / 1 clean exit**, unchanged from **34 / 24 / 1**.
The explicit Metal acceptance suite is separate: default-corpus menu/console
frames neither establish nor disprove Metal scene correctness.

Static shader coverage, identical audit options at `3deb0fb` and this follow-up:
**16 embedded DKSH programs, 521 instruction occurrences, 514 decoded (98.66%),
8 translated (50%)**, unchanged. Seven unknown occurrences remain: three RRO
(`5c90`) and four tessellation (`efd0`). Additional failures include resource
mapping and the vertex instance-ID input. This static audit does **not** cover
every runtime-generated OpenGL shader, and translation success is not native
execution acceptance. The requested full runtime shader census and fixing every
unsupported opcode remain unfinished.

Remaining work: Example05 tessellation and smoothed wide wireframe; Example06–07
multisample storage/resolve; Example08 viewport selection and multipass; Example09
QMD dispatch integration and shader support; multi-instance scene acceptance;
textured_cube/lenny RRO; dynamic_resolution/gpu_console MME; SDL2 mapping kind
0x02. See `../gpu/remaining/README.md`. No speculative register or opcode no-ops
were added. No input binaries, proprietary runtime material or keys are committed.

## Completed changes

- `eed298d`: fixed-clock, first-presentation capture mode and presentation-gated
  input transitions. Example04 includes sharp texture transitions; tolerance is
  unchanged. The controlled clock removes the framebuffer-setup tick delta and
  wall-time warmup dependency documented in `../gpu/accepted/README.md`.
- `822b4aa`: accepted deko_basic with integer vertex IDs and bounded dynamic U32
  constant loads. Six native scenes now have source-derived pixel checks and PNGs.
- `4fe30b9`: bounded audren PCM16 renderer through the existing CoreAudio path.
  The original corpus example produces nonzero callback samples and exits cleanly.
- `634bb73`: retained/validated HID hold orientation; SDL reaches GPU setup.

## Verification

`gpu-load/` records ten consecutive loaded passes of the five-scene regression
suite: 50 pixel-test passes and byte-identical Example04 images. Four load workers
consumed 2,030.065 CPU-seconds. The expanded six-scene suite also passed separately.

- `bash scripts/test-switch-gpu-accepted.sh`: six passed, no ignored tests.
- `cargo test --offline --manifest-path tools/switch/core/Cargo.toml --features desktop --lib`:
  107 passed. The final added channel-change rejection also passed the three
  focused renderer tests.
- `cargo test --offline --manifest-path tools/switch/shader/Cargo.toml`:
  46 passed, including native Metal tests and indexed-load/vertex-ID coverage.
- `cargo test --offline --manifest-path tools/switch/gpu/Cargo.toml`:
  35 passed, five existing ignored tests; native tests passed.
- `bash scripts/test-switch-audio.sh`: corpus audren and synthetic audout passed.
- `SOJU_SWITCH_PROOF="$PWD/dist/corpus-final" bash scripts/test-switch-examples.sh`:
  all 59 local NROs run with the same default backend and first-frame budget as
  the earlier corpus measurement. **34 frames / 24 stops / 1 clean exit**, versus
  **33 / 25 / 1**. audren-simple moved from stop to frame. This counts console/menu
  presentations; it is not graphics-scene acceptance. See `corpus-59.txt`.

## Outstanding requested graphics work

The requested remaining graphics expansion is incomplete. Example05 tessellation,
Example06–07 MSAA, Example08 higher viewports/multipass, Example09 compute, new
multi-instance pixel acceptance, and the remaining GL/SDL scenes are not accepted.
`../gpu/remaining/README.md` and its ordered method outcomes identify current
boundaries. No image or timeout from these probes is presented as acceptance.

The installed corpus has no separate EGL or instancing NRO. Audio coverage is
limited to the documented 48 kHz PCM16 final-stereo-mix profile; other renderer
formats/effects remain unsupported. No input NROs or proprietary runtime material were added to the repository.
