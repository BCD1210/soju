# Brood War M18 checkpoint

M18 qualification is incomplete. Required XR runs are blocked by the active D2R
qualification job. Started at `815337a` on `feat/broodwar-3d`.
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

`population-desktop-final/` measured **44.982 seconds** following a
30-second warmup. All nine native samples retained **125 owned units**. All
120 movement orders were accepted; **119 Marines displaced more than 32 pixels**.
One Marine moved less than that threshold. The default texture budget recorded
**zero rejections** and a **1,045.92 MiB** peak.

| Stage | p50 ms | p95 ms |
|---|---:|---:|
| Bridge round trip | 31.66 | 44.81 |
| Native hook to bridge receipt | 9.53 | 17.09 |
| Bridge receipt to worker start | 6.82 | 10.26 |
| Worker start to packet publication | 9.35 | 21.52 |
| Native hook to publication (all displayed frames) | 28.09 | 59.29 |
| Packet publication to GPU completion | 12.00 | 40.68 |
| Packet publication to compositor presentation | 34.41 | 62.72 |
| Native hook to GPU completion | 45.54 | 75.23 |
| Native hook to compositor presentation | 68.08 | 97.63 |

Packet age meets 50 ms in this desktop run, but native-to-presented latency does
not meet 70 ms. Desktop cadence p95 was 23.83 ms with 2,223 estimated missed
90 Hz slots; this is not XR acceptance. The other qualification job remained
active; no private XR session was started.

The preceding `population-desktop-frame-admission/` run is retained separately:
packet p95 35.55 ms, native-to-GPU p95 66.41 ms, native-to-presented p95 87.92 ms,
and 1,023.71 MiB peak with zero rejections. It is not substituted for the latest
result. The new native-to-publication metric includes view/terrain republication
of older state on every displayed frame.

## Active-play desktop duel

`duel-desktop-active/` completed **609.427 measured seconds** after active warmup,
with 119 native samples and frames **743 → 13,839**. It retained **6–32 owned
units** throughout and ended with **eight workers, fourteen Marines, and an intact
base**, including four garrisoned Marines. Ten field Marines received an ordinary
attack-move order; native snapshots record their movement and engagement with
Hydralisks and defenses. Only three sampled snapshots contained visible opponent
units, so combat coverage was brief rather than continuous.

The default **1,536 MiB** limit recorded **zero rejections** and a **1,080.65 MiB**
peak. No viewer crash occurred. This passes desktop residency/stability only;
**XR qualification remains pending and desktop latency does not pass**.

| Active desktop duel stage | p50 ms | p95 ms |
|---|---:|---:|
| Bridge round trip | 36.47 | 73.52 |
| Native hook to bridge receipt | 3.04 | 25.98 |
| Bridge receipt to worker start | 1.89 | 4.10 |
| Worker start to packet publication | 8.58 | 20.49 |
| Packet publication to GPU completion | 11.82 | 54.94 |
| Packet publication to compositor presentation | 33.98 | 75.76 |
| Native hook to GPU completion | 36.11 | 173.78 |
| Native hook to compositor presentation | 58.44 | 193.39 |

Desktop cadence p95 was 21.86 ms with 30,089 estimated missed 90 Hz slots. It is
not evidence for the XR cadence gate. Stage percentiles must not be added; their
sampling populations differ as described above. The native-to-publication metric was added after this duel and is available
in the latest population run. It records source age on every displayed frame.

| Allocation owner | Desktop 125-unit peak MiB | Active desktop duel peak MiB |
|---|---:|---:|
| Unit/model textures | 329.52 | 348.05 |
| Terrain pages | 347.81 | 305.81 |
| Terrain gutters | 271.75 | 257.75 |
| Terrain sources | 32.81 | 28.22 |
| Terrain fog, including source masks | 7.31 | 7.98 |
| Cards/overlays | <0.01 | 74.06 |
| Effects | 0 | 32.30 |
| Render targets | 104.75 | 104.75 |
| Captures | 0 (disabled) | 0 (disabled) |

Evidence includes `live-final-metrics.json`, `live-state-samples.json`,
`warmup-state-samples.json`, full native snapshots, `live-orders.json`,
`raid-orders.json`, `raid-after.json`, and the exact executed `driver.py`.

Two earlier duel attempts are retained and are not passes. `duel-desktop/` ran
610.076 seconds, but lost all owned units by 368.89 seconds and continued with a
lighter workload; its peak was 983.39 MiB with zero rejections.
`duel-desktop-defense/` failed after about 505 seconds when all owned units were
lost, peaking at 1,044.38 MiB with zero rejections. Neither was a renderer crash.

The original driver waited through viewer warmup before issuing mining orders.
The active driver now plays throughout warmup, prioritizes the bunker and four
Marines before worker expansion, reserves repair workers, and delays scouting.
It arms metrics only after 30 connected seconds and then requires another
610 seconds without losing all owned units. These are ordinary game commands,
not renderer tuning overrides. The next XR run uses the same active-warmup
strategy and schedules its field assaults within the driver.

## Validation

- Python: **286 passed, 60 subtests passed**, `tests-python-freshness.log`.
- Rust: **17 passed**, `tests-rust-frame-admission.log`.
- Runtime: **195 desktop / 200 XR / 200 fallback**, `runtime-*-freshness.json`.
  These checks ran offline against the private duel asset selection.
- Rendered regression: **16 normal views pass, all 80 fault controls rejected**,
  `rendered-batched-cliff/acceptance.json` and
  `rendered-batched-slope/acceptance.json`.
- `build-evidence.json` records executable/module hashes and confirms staged Swift
  sources match the repository. No game assets or `tools/broodwar_native/target/`
  are staged. `asset-upgrades/` and shared runtime source/services are untouched.

## Local implementation commits

- `eab5351`: deduplicate terrain residency and bound cache eviction work.
- `e0f3e98`: pipeline scene deltas and measure native display latency.
- `b148106`: reuse animated geometry and trim scene packet construction.
- `11f45cd`: overlap bridge reads with bounded worker admission.
- `ca157e3`: batch terrain uploads and expose packet display latency.
- `794451f`: return to simulation after each snapshot read.
- `31fbf9c`: expose native-state age at packet publication.

`acceptance.json` explicitly records incomplete status. The other qualification
job still owns the simulator. Only desktop viewing and
offline XR runtime checks have run so far; private XR controller chains, stress,
and the full duel await release of that job.
