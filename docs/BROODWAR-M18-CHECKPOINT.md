# Brood War M18 checkpoint

M18 qualification is in progress. Started at `815337a` on `feat/broodwar-3d`.
Evidence is private under `runtime/3d-proof/m18/`, where `runtime/` means
`~/.local/share/soju/broodwar-native/`. All commits remain local.

## Default texture residency

The default remains **1,536 MiB**. Terrain page arrays previously copied one
complete mip chain per map tile, including repeated artwork. Pages now deduplicate
identical diffuse textures while keeping separate fog slices and vertex indices.
Original resolution, gutters, filtering, and geometry are retained. An exploratory
lower-resolution path was discarded before committing.

Texture leases report current and peak bytes by allocation owner. The limits still
count resources pinned by scenes and in-flight GPU submissions, independently of
cache membership. LRU lookup, promotion, replacement, and eviction now use constant
time links; the previous eviction scanned every entry. Unchanged terrain batches
are reused by revision.

All three desktop command fixtures completed a 30-second warmup and 35-second
observation with captures enabled, the default budget, and **zero rejections**.
These are residency checks; simulator controller chains are a separate gate.

| Desktop fixture | Total peak MiB | Unit/model peak | Terrain pages peak | Terrain fog peak | Captures peak | Rejections |
|---|---:|---:|---:|---:|---:|---:|
| Terran | 620.75 | 485.89 | 10.36 | 23.02 | 5.45 | 0 |
| Zerg | 518.18 | 380.64 | 10.36 | 22.97 | 5.45 | 0 |
| Protoss | 630.49 | 491.75 | 10.36 | 22.92 | 5.45 | 0 |

Each fixture also retained up to 104.75 MiB in render targets, 0.25 MiB in terrain
gutters, and 0.09 MiB in terrain source textures. Cards/overlays peaked at
<0.01 / 2.72 / <0.01 MiB respectively. Owner peaks occur at different times and must
not be added to reconstruct the total peak. Runtime swapchains and framework-private
allocations are outside this renderer-owned accounting.

Evidence: `desktop-{terran,zerg,protoss}/final-metrics.json`, `samples.json`,
`launch.json`, `diagnostics.json`, and captures. Early exploratory runs are retained
under their original names and `desktop-prebatch-{race}/`; they are not final
performance acceptance.

## Native frame and display timing

The native game-thread bridge timestamps each response frame. Scene, terrain,
and evaluated command cards can now return in one response from that frame;
`batched-bridge-check.json` verifies matching scene/terrain frame numbers. Polling
changes from 100 ms to 5 ms. A bridge read can overlap worker construction;
worker admission limits the queue to one pending native update.
Native events remain FIFO. Unit deltas publish before terrain refinement, and
unchanged view packets are coalesced. Changed terrain pages upload in one GPU batch per refinement. Fog source
textures are charged to terrain-fog. Terrain pick geometry is reused by revision;
unwarped identical unit frames share pick meshes. Height interpolation avoids
per-sample temporary arrays without changing the surface calculation.

Metrics correlate native frame time, bridge receipt, worker start, packet
publication, and GPU completion. Desktop also records compositor `presentedTime`.
XR converts the runtime's predicted display time through the standard timespec
extension and reports a display estimate using the later of that time and GPU
completion. This is **not physical photon telemetry** and does not measure the
simulator's downstream decode/compositor or a physical headset display.

Per-stage distributions use the first displayed sample of each native timestamp;
packet age and end-to-end distributions include every measured rendered frame.
The native wall timestamp is converted to the host monotonic clock at receipt.
Frozen snapshots without a native timestamp do not count as live latency samples.

Final 125-unit and ten-minute XR latency/cadence tables are pending.

The first clean desktop population attempt crashed during warmup, before its
measurement marker. `population-warmup-crash.ips` records geometry-source
extraction on the scene worker while the main thread was flushing a SceneKit
transaction. This run is retained in `population-desktop-warmup-failure/` and is
not a pass. A construction transaction uses SceneKit's global lock. Three subsequent
population runs completed without this crash; the stack establishes the failure
location, not a proven root cause.


Desktop profiling found repeated parsing of SceneKit shader modifiers in the
Metal material path, repeated mesh index property calls, and cache entries that
retained old material textures. The Metal path now avoids unused shader parsing,
shares immutable vertex sources with separate mutable geometry elements, and
caches geometry capacity without retaining old nodes or texture maps.

The 45-second moving population run at `population-desktop-hotpath/` retained
125 owned units throughout, with zero allocation rejections and a 1,053.48 MiB
total texture peak. Its worker p95 fell to 25.04 ms from 86.55 ms in
`population-desktop-shared/`. Packet age p95 was 39.09 ms, but native-to-presented
p95 was 118.27 ms and desktop cadence p95 was 25.79 ms. **This is not a latency or
cadence acceptance pass.** It ran while the other qualification job was active.
The next revision overlaps bridge reads and worker construction. Its follow-up
`population-desktop-pipeline/` run confirmed movement of all 120 Marines and zero
rejections, but failed timing gates (packet p95 173.69 ms, native-to-presented
p95 383.28 ms). The captured concurrent process load includes D2R and other
compilation. This does not establish causation or a passing isolated result.
Final XR results must be measured separately.

A native-only sequential probe isolated repeated snapshots within one frame hook:
80 reads returned 78 distinct frame numbers, and one frame was returned three
times with age growing to 342.77 ms. A pipelined reader could queue another
request while the native sender was descheduled, keeping the hook in its request
loop instead of returning to simulation. The hook now returns after a snapshot
read while retaining the existing bounded command handling.

| Native-only 80-read probe | Distinct frames | Age p50 ms | Age p95 ms | Age max ms |
|---|---:|---:|---:|---:|
| Before frame admission | 78 | 17.21 | 141.18 | 342.77 |
| After frame admission | 80 | 8.73 | 13.82 | 25.96 |

Evidence: `native-latency-probe.json` and
`native-latency-frame-admission.json`. These are nearest-rank percentiles; the initial console probe used the next
rank. This probe is not display acceptance.

## Latest desktop population check

After frame admission was corrected, `population-desktop-final/` measured
**45.043 seconds** following a 30-second warmup. All nine native samples retained
**125 owned units**, and all **120 Marines moved more than 32 pixels**. The default
texture budget recorded **zero rejections** and a **1,023.71 MiB** peak.

| Stage | p50 ms | p95 ms |
|---|---:|---:|
| Bridge round trip | 31.19 | 42.05 |
| Native hook to bridge receipt | 7.31 | 9.80 |
| Bridge receipt to worker start | 5.99 | 8.44 |
| Worker start to packet publication | 9.48 | 20.74 |
| Packet publication to GPU completion | 5.86 | 35.55 |
| Packet publication to compositor presentation | 27.32 | 57.60 |
| Native hook to GPU completion | 45.80 | 66.41 |
| Native hook to compositor presentation | 67.69 | 87.92 |

Packet age meets 50 ms in this desktop run, but native-to-presented latency does
not meet 70 ms. Desktop cadence p95 was 21.69 ms with 2,220 estimated missed 90 Hz slots;
this is not XR acceptance. The other qualification
job remained active; no private XR session was started.

## Desktop duel stability check

`duel-desktop/` completed **610.076 measured seconds**, 121 native samples, and
13,780 native-frame advances without a viewer crash. Texture peak was
**983.39 MiB**, with **zero rejections** under the default limit. However, the
scripted defense lost its workers and lifted its command center; all owned units
were gone by 368.89 seconds. There were 27 samples with visible opponent units.
This is **not a full live-duel acceptance pass**, and it does not replace the
required XR duel. The later portion was a much lighter workload.

| Desktop duel stage | p50 ms | p95 ms |
|---|---:|---:|
| Bridge round trip (overlaps construction) | 36.71 | 52.35 |
| Native hook to bridge receipt | 2.27 | 18.74 |
| Bridge receipt to worker start | 1.13 | 3.90 |
| Worker start to packet publication | 3.97 | 13.28 |
| Packet publication to GPU completion | 20.08 | 50.04 |
| Packet publication to compositor presentation | 42.40 | 71.53 |
| Native hook to compositor presentation | 55.96 | 104.51 |

| Allocation owner | Latest desktop 125-unit peak MiB | Desktop duel peak MiB |
|---|---:|---:|
| Unit/model textures | 314.55 | 310.56 |
| Terrain pages | 318.23 | 289.41 |
| Terrain gutters | 268.75 | 246.00 |
| Terrain sources | 33.28 | 29.91 |
| Terrain fog, including source masks | 7.05 | 7.51 |
| Cards/overlays | <0.01 | 54.08 |
| Effects | 0 | 15.39 |
| Render targets | 104.75 | 104.75 |
| Captures | 0 (disabled) | 0 (disabled) |

The second attempt, `duel-desktop-defense/`, failed after about 505 seconds when
all owned units were lost. Its peak was 1,044.38 MiB with zero rejections; the
failure was not a renderer crash. The original driver waited through renderer
warmup before issuing any mining orders, giving the opponent a head start.

`measure_duel_active.py` now drives ordinary native game commands throughout
warmup, arms metrics only after 30 connected seconds, then requires another
610 seconds without losing all owned units. It prioritizes the bunker and four
Marines before worker expansion, reserves repair workers, and delays scouting.
The active-warmup attempt is still in progress; these are not tuning overrides.

## Validation

- Python: **286 passed, 60 subtests passed**, `tests-python-frame-admission.log`.
- Rust: **17 passed**, `tests-rust-frame-admission.log`.
- Runtime: **195 desktop / 200 XR / 200 fallback**, `runtime-*-batched.json`.
  These checks ran offline against the private duel asset selection.
- Rendered regression: **16 normal views pass, all 80 fault controls rejected**,
  `rendered-batched-cliff/acceptance.json` and
  `rendered-batched-slope/acceptance.json`.
- `build-evidence.json` records executable/module hashes and confirms staged Swift
  sources match the repository. No game assets or `tools/broodwar_native/target/`
  are staged. `asset-upgrades/` and shared runtime source/services are untouched.

## Local commits

- `eab5351`: deduplicate terrain residency and bound cache eviction work.
- `e0f3e98`: pipeline scene deltas and measure native display latency.
- `b148106`: reuse animated geometry and trim scene packet construction.
- `11f45cd`: overlap bridge reads with bounded worker admission.
- `ca157e3`: batch terrain uploads and expose packet display latency.
- `794451f`: return to simulation after each snapshot read.

The other qualification job still owns the simulator. Only desktop viewing and
offline XR runtime checks have run so far; private XR controller chains, stress,
and the full duel await release of that job.
