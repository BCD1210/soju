# M12 pacing and private-match checkpoint

The private-match launcher now reaches advancing Terran-versus-Zerg computer games unattended. CPU stall fixes and renderer texture ownership limits are implemented. The full cadence, geometry, allocation and total texture residency requirements remain open; this checkpoint is not overall acceptance.

Private traces, captures, launch records, tables and test logs are under `~/.local/share/soju/broodwar-native/3d-proof/m12/` (`runtime/3d-proof/m12/`). Game assets remain outside git. Dots's `asset-upgrades/` was read-only; the inbox was empty at inspection. No D2R process or shared runtime service was stopped. The existing XR simulator was retained.

## CPU investigation and changes

Points of Interest signposts cover the frame loop, runtime wait, native bridge reads, state application, world ticks, stream lookup/publication, billboard updates, batch preparation, eye encoding and GPU ring waits. Time Profiler recordings and exported samples are retained in `baseline-time-profiler.trace`, `live-time-profiler.trace` `cached-time-profiler.trace` and `final-time-profiler.trace`.

The live profile showed state application at **27.15 ms p95**, batch preparation spikes of **22.95 ms**, and negligible GPU ring waits (**0.0014 ms p95**). Bridge reads reached about **49 ms p95**, on the background request queue. Main-thread stacks exposed repeated creep-array bridging, transient arrays during vertex packing, and native batch rebuilding. Changes cache typed creep/tile metadata, pack vertices without per-vertex temporary arrays, reuse unchanged native group buffers, and skip identical terrain/fog/creep updates. Changed fog still invalidates terrain; a runtime regression check verifies hidden tiles are removed.

After group-buffer reuse, a separate profile measured batch preparation at **0.216 ms p95 / 0.892 ms maximum**. State application still reached **20.77 ms p95**, motivating the subsequent terrain-packet cache. These are different live sampling windows, not a controlled benchmark or proof that all stalls are removed. Streaming publication and scene mutations still use the main thread.

Diagnostics serialize/write on a utility queue. Hidden overlay labels no longer rebuild. XR declares user-interactive quality of service and a latency-critical activity. LaunchServices starts the measurement viewer at nice **0**; command-runner children inherited nice **5**, and those earlier runs are identified separately. No global process priority or shared runtime settings were changed. An isolated 1,000-iteration benchmark of all seven histogram summaries measured 0.013 ms p95 / 0.107 ms maximum, which does not explain the observed multi-millisecond stalls; it is not a live contention trace. A private wait-primitive probe did not improve tail latency, so no speculative runtime pacing replacement was installed.

## Launcher and live evidence

`play --restart --detach --map <private-map>` performs bounded dedicated-bottle shutdown, matches only this installation's loader/game paths, verifies the requested map and advancing native frames, then returns. Detached launches use a separate process session and private log streams. An initial retry exposed that inherited launcher streams could kill the game after apparent startup success; that failure is preserved in `live-detach-failure/` and the detachment fix has a regression test.

`make_duel_fixture.py` derives a two-player scenario from an owned two-start map, preserving terrain/resources. It places Terran and Zerg starting bases/workers, initial resources and the built-in Zerg expansion script. Generated maps and provenance hashes are private. The land-map run uses owned Benzene terrain; the first complete run used Midnight Lagoon. The measurement drivers exercise resource gathering, production and scouting. These workloads do not establish large-army or late-game performance.

The first complete live run measured **610.091 seconds**, **15.32 ms cadence p95**, **7.73 ms stereo CPU p95**, **9.65 ms stereo GPU p95**, and **1,592 estimated missed refresh slots**. It predates group-buffer reuse and terrain-packet caching and used inherited nice 5. Final land-map measurements are recorded in the private `timings.md` and `acceptance.json`. Workloads and process scheduling differ, so these runs must not be presented as a direct speedup ratio. CPU and GPU overlap. Missed slots are cadence estimates, not compositor telemetry.

Final land-map run: **609.985 seconds**, **22.47 ms cadence p95**, **17.87 ms stereo CPU p95**, **9.47 ms stereo GPU p95**, and **6,625 missed slots** (108.6 per ten seconds). Native frames advanced from **1,335 to 15,127**. Owned units remained present throughout, grew from 5 to 40, and the maximum visible population was 65. The scout observed Zerg economy, tech and combat units. The final renderer texture peak was **1,203.375 MiB**, below the unchanged 1,536 MiB cap, with **zero rejected allocations**.

An intervening attempt lost the Terran base and is preserved in `live-defeat/`; empty-scene time is not accepted. The final private controller reassigns idle miners, builds depots/barracks/bunker, trains and garrisons Marines, repairs the bunker and can lift a critically damaged Command Center. `measure_live_native.py`, `live-orders.json` and `live-scout-followup.json` retain the exact controller and orders in the private evidence directory. This is a small test controller, not a competitive player. Acceptance now requires owned units throughout and an observed opponent, in addition to advancing frames and duration.

The post-run 25-second diagnostic (`final-signposts-summary.json`) measured billboard updates at **20.32 ms p95**, batch preparation at **17.21 ms p95**, and state application at **32.35 ms p95**. Main-thread samples identified repeated frame-catalog bridging, with about 3.7 seconds inclusive in array conversion. A subsequent change caches typed frame records per sprite decoder, avoiding repeated catalog conversion during view updates. That change is later than the completed ten-minute run; the preserved run hash and its numbers are not presented as final-build certification.

Both cadence requirements **fail**. Full terrain-array rebuilding on fog changes and live native geometry changes still cause main-thread work. The ten-minute measurement is complete; performance is not accepted.

## Geometry and memory

`--grounding-job` produces four fitted camera views for each native completed building image chain. The private gallery has **94 rendered building entries / 376 captures**, plus a coverage table and five contact sheets. Type 214 is a non-rendered start-location marker. Empty image chains remain explicit gaps. These are SceneKit flat-ground geometry diagnostics, not XR or slope-contact certification. Minimum-vertex contact remains insufficient for full footprint grounding. Cliff stretching is unresolved; `cliff-diagnostic.json` quantifies raw neighbor relief stretch before smoothing and is explicitly not a rendered-image acceptance test.

A shared **1,536 MiB** renderer texture budget follows each Metal texture's actual lifetime through cache, scene, batch and texture-view owners. It reserves heap-aligned storage, checks allocated size, rejects over-budget creation, and releases ownership on deallocation. `SOJU_BW_TEXTURE_MIB` sets the limit. Runtime checks cover rejection, release and retention through a texture view. The first capped land-map run exited after about seven minutes when newly observed units exhausted residency and a terrain upload was rejected; evidence is preserved in `live-residency-failure/`. The native group cache had charged only buffer bytes while retaining textures, and terrain textures had no eviction bound. Group entries now charge retained textures too, and terrain texture retention uses a 256 MiB LRU. The global cap was not raised. Runtime swapchains and framework-private textures remain outside this accounting, so this is not a total-process texture residency guarantee.

Per-vertex temporary arrays and repeated mesh buffers were removed, but zero per-frame allocations is not established. An Allocations capture stalled and produced an invalid trace; the export error is retained. Command buffers, SceneKit work, dispatch closures, diagnostic snapshots and live scene changes still allocate. The acceptance report keeps allocation and total-residency gates false.

## Verification and commits

- Optimized desktop, XR default and XR SceneKit fallback: **119 runtime checks passed** each.
- Asset-venv Python: **282 passed, 60 subtests passed**; dedicated launcher/map checks: **5 passed**.
- Rust: **14 passed**.
- `5dc5d1b`: frame/bridge/streaming signposts.
- `c12f344`: vertex packing, creep metadata, diagnostics and XR scheduling.
- `83fba6f`: scoped recovery and private duel preparation.
- `7f94215`: lifetime-based renderer texture budget.
- `e491d46`: native group-buffer reuse.
- `e8f7516`: unchanged-terrain suppression and typed tile metadata.
- `7e69daf`: detached client session and output streams.
- `e48dc99`: four-angle building catalog diagnostics and acceptance reporting.
- `937bf2d`: charge cached texture owners and bound terrain texture retention.
- `f0a6dd1`: cache typed frame catalogs for billboard updates (after the ten-minute run; not yet re-measured live).

Run `tools/broodwar_native/report_m12.py` with asset-venv Python to regenerate the private timing table, building sheets and explicit acceptance record. The final launch record preserves the measured binary hash.
