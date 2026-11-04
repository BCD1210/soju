# Continuation results

Base: `4c4432e`; implementation through `e40c9db`, test cleanup `aef3424`.
All continuation commits remain local; nothing was pushed.

**Partial completion:** corpus 44/59 frames, shader audit 14/16, and p99 still
above 17.5 ms on es2gears and dynamic_resolution. Live dirty-tracked guest-memory
sharing, asynchronous attachment visibility and a display-linked swapchain are
not implemented in this continuation.

## Changes

| Commit | Change |
| --- | --- |
| `ed4d559` | AS-GPU unmap retires split views and preserves reservations |
| `5d42fa2` | Consistent virtual desktop mains-power queries |
| `f89b7b4` | Four missing square ASTC sRGB variants and nonblack sample oracle |
| `18dd8eb` | Bounded reusable depth staging, direct color Data capture, preallocated row packing |
| `e40c9db` | Ignore inactive sampler comparison functions; textured_cube presents |
| `aef3424` | Remove redundant single-element test loop for strict lint |

The depth staging pool is bounded by 32 MiB and one eighth of the configured
cache budget at retention time. Scratch resources count toward native cache
accounting and are released first under cache pressure. Reuse occurs only after
GPU completion and copying into independently owned output bytes. The change
removes allocation churn and one color copy, not synchronization or guest writeback.

## Frame times

Each row contains 300 headless presentations; mean and nearest-rank percentiles
use the 299 subsequent intervals. The baseline binary was `5d42fa2` (its rendering
path is unchanged from `4c4432e`). Final runtime code is `e40c9db`. The final two
runs were serial with no concurrent builds/tests from this session. Application
caches were warm; first-present times include launch/load. These are short runs,
not a soak. Headless throughput excludes the window compositor and display pacing.

| Title / run | Mean ms | p95 ms | p99 ms | >17.5 ms | First present ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| es2gears / before | 13.967 | 14.445 | 22.266 | 4/299 | 464.816 |
| lenny / before | 10.044 | 10.468 | 11.500 | 1/299 | 467.120 |
| dynamic_resolution / before | 23.351 | 25.205 | 31.787 | 299/299 | 506.828 |
| 2048 / before | 1.218 | 1.365 | 1.694 | 0/299 | 359.864 |
| es2gears / final-1 | 12.576 | 12.993 | 19.607 | 4/299 | 475.190 |
| lenny / final-1 | 8.639 | 9.043 | 11.603 | 1/299 | 460.211 |
| dynamic_resolution / final-1 | 19.890 | 20.711 | 26.360 | 299/299 | 512.660 |
| 2048 / final-1 | 1.257 | 1.395 | 1.827 | 0/299 | 359.091 |
| es2gears / final-2 | 12.635 | 13.108 | 19.712 | 4/299 | 482.474 |
| lenny / final-2 | 8.723 | 9.101 | 14.507 | 1/299 | 454.416 |
| dynamic_resolution / final-2 | 19.951 | 21.817 | 28.243 | 299/299 | 508.695 |
| 2048 / final-2 | 1.364 | 1.633 | 1.996 | 0/299 | 437.773 |

The 2048 run is an idle, unpaced rendering probe with no moves injected; its
sub-2 ms intervals do not represent display refresh or interactive gameplay.
An exploratory after run overlapped later build activity and is excluded from
the final table. It remains under ignored `dist/continuation/timing-after/`.
Raw included samples: [frame-times.json](frame-times.json); aggregates:
[timing-summary.json](timing-summary.json).

## Compatibility and validation

- Default 59-title corpus: **42 frame / 16 stop / 1 exit0 → 44 / 14 / 1**.
- All 16 initial stop reasons and current boundaries: [stop-reasons.md](stop-reasons.md).
- Power and textured_cube newly present; deko_console advances to `0x113c=2`.
- 125 desktop-feature core library tests pass.
- 29 active Metal render tests pass with API validation; 5 existing ignored tests remain ignored.
- Native cache test passes, including new padded-row, staging reuse, capture ownership and eviction checks.
- All 10 accepted source-derived scene tests pass with unchanged oracles.
- 10 GPU state tests pass, including inactive comparison-function coverage and active-mode rejection.
- All 7 texture tests pass, including all Metal format codes checked against the installed SDK.
- Texture sampling oracle checks 44 formats/views (45,056 pixels). All six supported square ASTC sizes now test linear and sRGB mid-gray values.
- Strict core and GPU all-target clippy pass. The initial core lint failed on a single-element test loop; `aef3424` corrects it.
- Fresh symbolic-texture shader audit: 521/521 instruction occurrences decoded, 14/16 programs translated.

The default corpus is a first-presentation metric with a 15-second title budget.
The textured-cube capture was visually inspected, but no new source-derived cube
oracle is claimed. New service behavior is based on the public libnx command
wrappers recorded in the core source ledger; no unavailable service is silently
accepted to raise corpus counts.

## Remaining gaps

| Requested area | Current limit |
| --- | --- |
| Attachment synchronization | Still synchronous; full color/depth guest-layout writebacks remain |
| Live guest memory | Snapshot comparisons and immutable capture buffers remain; no hardware dirty-page tracker |
| Display pacing | Existing SDL Metal vsync plus guest-loop timer remains; no new asynchronous swapchain or CAMetalDisplayLink |
| Corpus ≥50 | 44 frames; 14 explicit stops remain |
| Tessellation 16/16 | 14/16; control/evaluation stage contracts and patch ABI absent |
| MSAA + resolve | Multisample storage/layout, pipeline identity and resolve path absent |
| Example09 | No new compute-to-raster acceptance; previous polygon-smoothing boundary remains unresolved |
| BC1–7 | Existing native Metal path exercised by the format test; broader guest descriptors/mips are not covered |
| ASTC | Six square footprints, linear/sRGB; rectangular footprints and broad descriptor/mip support remain |
| sRGB/depth-stencil | Existing native format tests pass; no universal format/descriptor coverage claim |
| Transform feedback | Still rejected; output capture and counters absent |

The prior Example05/06/07/09 feature probes were not rerun in this continuation;
their implementation gaps were inspected and are not counted as new acceptances.

## Reproduce

Build/sign an isolated runner using `scripts/test-switch-examples.sh` or
`scripts/test-switch-gpu-accepted.sh`. For each locally supplied public probe,
use a fresh canonical home directory and run:

```sh
SOJU_SWITCH_HOME=<canonical-test-home> <signed-runner> --headless <probe.nro> \
  --frames 300 --capture <capture.png> --frame-times <times.json>
```

Unset profiling/GPU-audit overrides and run timings separately from builds and
tests. Discard sample zero for steady-state summaries. The exact corpus
summaries are [before](corpus-before.txt) and [after](corpus-after.txt).
Full local validation logs and captures remain ignored under `dist/continuation/`.
