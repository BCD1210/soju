# Frame-rate results

Code: `78d4dc7`, following baseline `304f2b3`. All changes are local commits;
nothing was pushed. Measurements use the M4 Pro headless runner and locally
supplied public es2gears, with the Metal backend enabled.

## Result and limits

| Metric | Before | Final |
| --- | --- | --- |
| Subsequent frame interval | Reproduced 6741.148 ms mean, two intervals | 15.344 / 14.567 ms mean, two 600-frame runs |
| Subsequent frame rate | 0.148 fps reproduced; user reported 0.13 fps | 65.172 / 68.650 fps |
| First present | User reported 8.33 s; reproduced guest-loop-only 7.470 s | Launch-inclusive 0.807 / 0.601 s, warm application caches |
| p95 interval | Two intervals are insufficient | 19.796 / 16.929 ms |
| p99 interval | Two intervals are insufficient | 30.892 / 22.736 ms |
| Cold application-cache first present | Not measured | 1.156 / 0.848 / 0.721 s |
| Native submissions/readbacks | 281 pairs per frame | 122 pairs over 120 frames |
| 64-worker stress median | 455.768 ms parallel vs 324.540 ms serial | 328.024 ms adaptive vs 329.457 ms serial |

The warm runs meet average 60 fps and sub-second first present. They do **not**
establish locked 60 Hz pacing: 78/599 and 32/599 intervals exceeded 16.667 ms.
Cold startup is not reliably below one second. These are two roughly ten-second
runs, not a long soak or proof of commercial-title performance. First-present
timing ends when the headless runner receives a real rendered display frame;
window compositor/swapchain latency is not included. Cold tests use fresh worker
and pipeline cache directories; the operating system's compiler cache was not
flushed. Raw intervals and launch timings are in `final-results.json`,
`final-frame-times-{1,2}.json`, and `application-cold.json`.

## Per-stage profile

Values are inclusive milliseconds divided by presented frames: three baseline
frames versus 120 final frames. Initialization is amortized differently; these
columns are attribution measurements, not isolated steady-state microbenchmarks.
Nested stages overlap and must not be summed. Final counters are from a separate
warm, instrumented run, not either uninstrumented FPS run.

| Stage | Before ms/frame | Final ms/frame | Final calls |
| --- | ---: | ---: | ---: |
| Guest execution | 4.975599 | 0.189013 | 1803 |
| Host-memory lease acquisition | 0.014643 | 0.001409 | 4850 |
| SVC dispatch | 56.498515 | 1.422802 | 832 |
| IPC dispatch | 6916.939473 | 15.770926 | 535 |
| nvdrv ioctl dispatch | not separated | 14.762868 | 206 |
| GPFIFO/pushbuffer decode | 0.035861 | 0.019476 | 122 |
| MME interpretation | 0.001903 | 0.000072 | 5 |
| Guest mapping synchronization | 12.887292 | 2.578922 | 122 |
| Shader extent/instruction discovery | 16.309679 | 0.189066 | 33600 |
| Shader translation | not separated | 0.001108 | 2 |
| Draw/resource preparation | 2020.642428 | 0.967905 | 33720 |
| Draw-reference construction | not separated | 0.056301 | 31319 |
| Submission and completion boundary | 624.716503 | 2.690762 | 122 |
| Readback and guest-layout writeback | 4222.544876 | 4.961934 | 122 |
| Guest transaction write diff | 6.101944 | 1.125403 | 122 |
| Display pixel conversion | not separated | 0.836974 | 120 |
| Native worker startup | not separated | 0.191232 | 1 |

Final native counters separately recorded:

| Native stage | Before | Final total ms / calls | Final amortized ms/frame |
| --- | --- | ---: | ---: |
| Shader-pair source compilation | Included in submit | 1.300 / 1 | 0.010833 |
| Render pipeline creation | Included in submit | 0.260 / 1 | 0.002167 |
| Shared buffer imports | Included in submit | 21.186 / 2281 | 0.176551 |
| Texture uploads | Included in submit | 3.200 / 4 | 0.026670 |
| GPU completion wait | Included in submit | 102.941 / 122 | 0.857841 |
| Native readback | Included in readback | 237.380 / 122 | 1.978165 |

There were two shader translations, one shader-pair compilation and one render
pipeline creation across 120 frames. These counts did not grow per frame.
Pipeline archives and the exact embedded worker binary persist across launches;
the first process still creates Metal functions/pipeline objects. The baseline
predates the finer counters, so unavailable subdivisions are explicitly marked.
Lease acquisition time excludes time guest cores remain stopped while the host
owns memory. Its small value does not mean global GPU-submit rendezvous is solved.

## Instruments evidence

Time Profiler, signposts and Metal System Trace were actually recorded. Large
raw recordings/exports remain ignored under `dist/frame-profile/`:

- `time-before.trace`: baseline launch capture, normal exit, 23.187 s.
- `time-final.trace`: successful five-second Time Profiler attachment of the final
  runner. `time-attach.trace` also retains an intermediate post-optimization run.
- `signposts-after.xml`: 567,727 events filtered to subsystem `org.soju.switch`.
- `metal-final.trace`: successful final Metal System Trace attachment, three-second
  requested recording, with target completing its 600-frame run.
- `metal-final-application.xml`: 1,247 runner encoding intervals, 565.513 ms total.

Baseline Time Profiler leaf samples: target writeback 3,954 CPU ms, memory moves
3,161 ms, byte hashing 2,031 ms, deswizzle 809 ms, out of 19,554 sampled CPU ms.
Final five-second samples: memory moves 1,228 ms, comparisons 886 ms, target
writeback 223 ms, register address lookup 128 ms, out of 5,282 sampled CPU ms.
Different capture lengths/initialization coverage prevent direct total-time ratios.
Metal intervals are nested and filtered by runner process, not system-wide totals.
The baseline Metal capture included concurrent validation and is attribution-only.
Two unsuccessful trace attempts are excluded: launch-mode Time Profiler stalled,
and the first final Metal attachment exceeded its export timeout. The later
successful attachment is the evidence used here. Raw trace exports may include
system metadata; only runner aggregates are published in this report.

## Changes

The runner retains the Metal backend, pipeline state, checked GPU memory image
and shader discovery across submissions. Compatible draws are batched up to
ordered visibility/hazard boundaries. Normal readback uses an in-process callback
instead of frame files. Generation checks limit guest mapping copies and commit
diffs; fragment-backed guest mappings can be borrowed as checked segments.

Immutable vertex captures use aligned shared/no-copy Metal buffers and validated
geometric range reuse. Repeated draws reuse state and constants within checked
bounds. Retained native texture generations and bounded linear captures avoid
reimporting/reconverting their own output. Depth writeback preserves guest padding
without unpacking old values that will be overwritten. Normalization caches omit
only launch/upload bookkeeping already represented by actions and bindings;
resource discovery and hazard validation still run on every draw.

Registered NV fence events now support waits on already committed GPU work, so
longer runs no longer stop at the first such ioctl. Uncompleted/faulted future
waits remain explicit errors. The scheduler overlaps useful long guest quanta,
while cooperative SVC-heavy traffic avoids launching/cancelling sibling workers.

## Compatibility and validation

- Default corpus: **42 frame / 16 stop / 1 exit0**, unchanged title-for-title from
  the previous 42/16/1 result. No GPU/audit override was enabled for this check.
- All **10 accepted scene tests pass** without changing pixel oracles/tolerances.
- **2048 passes** default-mode moves/save/restart/exit. An earlier run failed its
  first unchanged-board input assertion; that failure is preserved in
  `2048-earlier-failure.txt`. A serialized retry and two later default runs passed.
  No claim is made that the earlier flake's root cause was fixed.
- **22 Hypervisor tests**, **51 NV tests**, and strict core/GPU lint pass.
- **29 active GPU render tests** pass with Metal validation. Native-cache-disabled
  tests also pass after capture reuse; the subsequent depth-loop change passed
  all 29 normal tests. Shared-capture ownership/padding checks pass.
- A deliberately corrupted isolated worker cache was replaced with the exact
  original binary and presented three frames (`cache-integrity.txt`).
- Contention: 100 alternating-order pairs, zero guest test failures, adaptive
  median 328.024 ms vs serial 329.4565 ms. This is parity within noise, not a
  material speedup or a promise about each individual run.
- CPU workload: median 316.216 ms adaptive vs 316.818 ms original parallel and
  923.489 ms serialized; 2.92x throughput with four cores overlapping.

## Remaining work

The full request is not complete. Full attachment readbacks/copies still occur
at guest visibility boundaries. Shared/no-copy buffers wrap immutable captures,
not the live guest arena. Snapshot synchronization still compares mapped ranges;
there is no hardware guest dirty-page tracker. GPU submission still holds an
exclusive host-memory lease and waits for completion. There is no new asynchronous
window swapchain/present path, and textured draws retain conservative boundaries.
Pushbuffer decode is measured and small; it was not replaced in this work.

Shader audit remains **14/16 translated with symbolic texture bindings** and
521/521 instructions decoded, not 16/16. Without symbolic bindings it is 11/16;
that is a different audit mode, not a regression. No tessellation, MSAA/resolve,
or Example09 raster acceptance is claimed. Fresh scene probes still stop at:

| Scene | Remaining rejection |
| --- | --- |
| Example05 | B197 method `0x320=0x201`; control/evaluation stage backend absent |
| Example06/07 | Multisample setup unsupported; sample storage/resolve not implemented |
| Example09 | Polygon smoothing `0x1658=1`; wide-line/smoothing raster work remains |

The next architectural step is safe asynchronous guest-memory ownership with
precise dirty/visibility tracking; average headless FPS does not substitute for
that or for the missing rendering features. No unsupported state was silently
accepted to improve the results.

## Reproduction

Build and sign an isolated runner using the existing accepted-scene script.
For uninstrumented throughput, set `SOJU_SWITCH_GPU=metal`, unset `SOJU_PROFILE`,
and run `--headless <local-es2gears.nro> --frames 600 --capture <output.png>
--frame-times <times.json>`. Compute `1000 / mean(times[1:])` for subsequent FPS;
use the printed `timing|first_present_ms` for launch-inclusive startup. Repeat with
`SOJU_PROFILE=1` separately for stage counters. Attach Instruments to an already
running process using `xcrun xctrace record --no-prompt --template 'Time Profiler'
--time-limit 5s --attach <pid> --output <trace>`; use `Metal System Trace` for GPU
attribution. Do not treat instrumented frame intervals as the throughput result.

`tools/switch/testtitles/benchmark_contention.sh` reproduces the 100-pair scheduler
comparison after building the local stress fixture. `scripts/test-switch-gpu-accepted.sh`,
`scripts/test-switch-homebrew.sh`, `scripts/test-switch-examples.sh`, and
`scripts/test-switch-hv.sh` retain their original acceptance checks.
