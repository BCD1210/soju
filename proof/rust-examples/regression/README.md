# Runner regression results

Branch: `feat/switch-core`. The requested shader tip `5e298df` was merged first
in `b6d6823`. All commits remain local.

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
