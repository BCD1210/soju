# Brood War M20 checkpoint

M20 starts from `1d08cde` on `feat/broodwar-3d`. The final measured source is
`c820ee1`. **One of three performance gates passes: possession. Desktop and
the ten-minute duel remain open.** This checkpoint does not claim completion.
Private evidence is under `~/.local/share/soju/broodwar-native/3d-proof/m20/`
(the native runtime's `3d-proof/m20/`). Game assets and build products are not
tracked. Shared OXRSys sources and services are not changed.

## Baseline and profiling

`stress-desktop-baseline/` reproduced the desktop failure: 45.03 measured
seconds, **78.39 ms actual compositor presentation p95**, 20.81 ms frame
interval p95, and zero texture rejections. Native-to-GPU p95 was 55.93 ms.
The source requested 90 fps and produced approximately 50 fps on this display.
Both desktop and XR already used direct native snapshot publication.

`stress-possess-baseline-2/` measured 44.86 seconds, **79.12 ms XR display-estimate
p95**, 20.10 ms worker-to-packet p95 and 15.46 ms bridge-to-worker p95.
The original selection attempt in `stress-possess-baseline/` failed before
measurement because another unit occluded the intended ray target.

Both CPU samples were taken after freezing the metric windows. The possession
sample showed one sample in camera follow, 2,754 samples under stream requests,
and 2,647 on the scene worker. The stream's reference-material construction
included 837 samples in relief material construction, mostly SceneKit image
conversion. Contact conformance was much smaller than in the pre-contact-grid
M19 profile. These are inclusive sampling counts, not additive stage timings.

## Implementation and intermediate results

- `690c856` introduced display-driven drawable delivery. Its initial desktop
  runtime suite passed 251 checks. `stress-desktop-display-link/` reached
  approximately 100 fps but **failed latency at 97.56 ms p95**. Increased frame
  rate alone did not solve queued presentation delay.
- `cd817ec` prepares Metal lighting maps directly, avoiding temporary SceneKit
  image properties. The fallback path retains reference-material construction.
  Runtime readback compares all five lighting maps with the reference pixels,
  dimensions and formats. Desktop/XR/fallback passed 252/257/257 checks;
  `build-direct-material-evidence.json` records matching staged sources.
- `stress-possess-direct-material/` is invalid: combat reduced the owned army
  from 125 to 121 units after map preparation was separated from viewer launch
  by investigation time. Its metrics cannot qualify the 125-unit gate.
- `stress-desktop-two-drawables/` failed before qualification. A pending MTKView
  callback requested a drawable after CAMetalDisplayLink assumed ownership.
  That experiment was reverted with the other display-link changes. The failure
  log is retained.

Final Python passed **286 tests plus 60 subtests**, Rust passed **18 tests**,
and desktop/XR/fallback runtime checks passed **266/271/271**. Evidence:
`tests-python-final2.log`, `tests-rust-final2.log`,
`runtime-desktop-final2.{log,json}`, `runtime-xr-cooperative-yield.{log,json}`,
and `runtime-fallback-final2.{log,json}`. `build-final-evidence.json` records
both binary hashes and zero staged-source mismatches against `c820ee1`.

## Further construction and scheduling work

`174c937` retains billboard poses across nonvisual native updates. Health/order
state still advances; actual position, visibility, elevation and appearance
changes invalidate the appropriate pose or geometry. `c1923ac` uploads and
picks typed relief meshes without converting them back out of SceneKit sources.
Runtime checks compare the resulting vertex, UV and index bytes.

`ca91f24` distinguishes demanded stream frames from speculative neighboring
frames. Prefetch completion alone does not republish the scene. A pending
prefetch promoted to demand still wakes publication exactly once. `806ef8c`
skips queued view work already consumed by native publication, while resource
readiness and terrain changes retain their own invalidation. Native snapshots
and event deltas are never skipped.

`6a44614` shares complete read-only Metal frame geometry. SceneKit fallback and
contact-warped geometry retain private construction objects. `2e0e05d` reuses
GPU buffers for immutable wrist-overlay geometry while publishing current pose,
opacity and material state. UV changes invalidate the cached upload. Tests
verify that retained packets preserve their old transforms. Desktop/XR/fallback
runtime checks passed **261/266/266** after these changes.

The following intermediate measurements are retained, including failures:

| Run directory | Seconds | Latency p95 | Interval p95 |
|---|---:|---:|---:|
| `stress-desktop-baseline/` | 45.03 | 78.39 ms | 20.81 ms |
| `stress-desktop-deadline-latch/` | 45.05 | 88.30 ms | 11.23 ms |
| `stress-desktop-display-link/` | 45.02 | 97.56 ms | 10.11 ms |
| `stress-desktop-mtk-double/` | 45.36 | 87.83 ms | 10.35 ms |
| `stress-desktop-refresh-rate/` | 45.22 | 97.77 ms | 10.54 ms |
| `stress-desktop-transaction/` | 45.05 | 88.25 ms | 11.05 ms |
| `stress-possess-baseline-2/` | 44.86 | 79.12 ms | 11.90 ms |
| `stress-possess-consumed-view/` | 45.23 | 76.08 ms | 11.93 ms |
| `stress-possess-demanded-stream/` | 45.36 | 74.53 ms | 11.92 ms |
| `stress-possess-pose-retention/` | 42.19 | 87.77 ms | 11.93 ms |
| `stress-possess-shared-geometry/` | 42.28 | 74.39 ms | 11.90 ms |

Desktop rows use actual compositor presented timestamps; possession rows use the
XR display estimate. CPU samples, when requested, start only after freezing the
measurement file. The display-link/refresh-rate experiments above do not qualify
the desktop gate. A vsync-disabled source experiment was saved privately but was
not measured or retained. Final desktop uses synchronized MTKView presentation.


## Final implementation

`e504b7e` avoids redundant SceneKit property writes during view refinement.
`8101fa3` reassembles native wire snapshots directly into bounded receive storage,
with validation for duplicate, mixed, truncated and out-of-order packets.
`d780b6f` and `c820ee1` let obsolete view-only construction yield when a native
snapshot arrives, including checkpoints inside geometry preparation. A yielded
packet does not advance its generation or replace the mailbox. Native updates
and event deltas are still processed; completed cache and terrain work can be
reused. Runtime checks exercise interruption, recovery and packet generations.

The possession profile implicated extra view/stream construction and queueing,
not the camera-follow arithmetic. A later sample contained about 1,502 inclusive
samples in view-only packet work versus 1,087 in native publication. The fixes
reduce that repeated work and let pending native publication preempt it. Camera
follow semantics, 48-pixel LOD threshold, rendering quality and the default
1,536 MiB texture budget are unchanged.

`32f2991` makes allocator tracing explicitly opt-in with
`--trace-frame-allocations`. Previously, every XR frame called all-zone allocator
statistics twice even during ordinary timing runs. Default latency, cadence,
residency and creation counters remain available; the slow report still samples
memory off the frame queue. Final runs report allocation tracing disabled and
zero allocation samples. This is **not** evidence of zero allocations.

`21a7f98` restores synchronized desktop pacing after unsuccessful presentation
experiments and adds separate drawable-wait and submit-to-presented metrics.
Desktop and XR both consume the direct native publication mailbox. Desktop
latency remains the actual drawable `presentedTime`, not GPU completion.

## Final 125-unit stress

All four runs below use default settings, nice 0, capture disabled, allocation
tracing disabled, no profiler and no overlapping builds. Every sampled owned
army contains 125 units; command acknowledgements and native movement are
verified. All three XR views retain fresh tracking and scene publication.

| Private run directory | Seconds | Native-to-display p95 | Cadence p95 | Missed XR slots | Texture rejections |
|---|---:|---:|---:|---:|---:|
| `stress-tabletop-final2/` | 42.83 | 64.92 ms | 11.89 ms | 0 | 0 |
| `stress-ground-final2/` | 43.88 | 68.00 ms | 11.87 ms | 0 | 0 |
| `stress-possess-final2/` | 45.91 | **68.11 ms** | 11.94 ms | 0 | 0 |
| `stress-desktop-final2/` | 45.04 | **87.79 ms — FAIL** | 20.87 ms | n/a | 0 |

XR uses the runtime display-time estimate; desktop uses actual compositor
presentation callbacks. Neither measures physical photons. Timing distributions
have different sample populations; their p95 values must not be added.

Desktop's final drawable wait is 0.05 ms p95. Submit-to-presented is 34.33 ms
p95 (27.08 ms mean), native-to-GPU 63.52 ms, native-to-packet 43.41 ms,
worker-to-packet 19.67 ms, bridge-to-worker 2.91 ms and native-to-bridge 11.63 ms.
The desktop requests 90 fps but presents at approximately 50 fps on this display
(20.01 ms mean interval). The remaining gate requires further reduction in
publication/presentation delay; it is not closed by the small drawable wait.
Vsync remains enabled. No quality reduction or asynchronous presentation is
used to qualify these results.

Further unsuccessful measurements are retained: desktop flush 87.77 ms and
view-yield 78.47 ms; possession overlay reuse 73.63 ms, property writes 76.10 ms,
and initial view-yield 66.61 ms with one missed slot. The first final possession
run at `32f2991` was **70.72 ms**, so it failed and prompted the deeper cooperative
checkpoints in `c820ee1`. Its tabletop/ground readings were 68.87/64.84 ms.
These earlier measurements are not substituted for the final same-build runs.

## Ten-minute duel

`duel-xr-final/` completed **609.99 measured seconds**, with 609.27 seconds of
native samples and 14,506 native frames advanced. It recorded 567 attacks,
526 damage events and 54 deaths. Every sample retained fresh tracking, fresh
scene publication and a surviving owned army. Peak texture residency was
1,320.08 MiB under the default 1,536 MiB budget.

- Cadence p95: **12.40 ms — FAIL** against 12.0 ms.
- Native-to-display-estimate p95: **84.77 ms — FAIL** against 70 ms.
- Missed refresh slots: **0**. Texture rejections: **0**.

The combat/survival qualification passes; the performance qualification fails.
Early lighter intervals passed but are not used in place of the complete run.
The run retains `process-preflight.txt` and
`process-midrun.txt`; the viewer and private simulator run at nice 0. The host
also has unrelated active applications, which were not stopped or changed.
This is recorded as environmental context, not a reason to discard a failed
measurement. `verify_final.py` combines the frozen stress measurements,
binary hashes and duel qualification into `final-qualification.json`, whose
`allPerformanceGatesPass` is explicitly `false`. The next work must address
combat-time scene publication/packet age and XR cadence, as well as desktop
presentation delay; the earlier M19 duel latency pass is not carried forward
as evidence for this build.

## Repository boundaries

Each implementation step is committed locally. Nothing is pushed. Game assets,
private fixtures, profiles and binary products remain outside tracked source;
`tools/broodwar_native/target/` is not staged. The existing edit to
`docs/BROODWAR-MODEL-QUEUE.md` is left untouched. Only the private broodwar
viewer/bottle and private simulator are used; shared OXRSys sources and services
are neither modified nor restarted.
