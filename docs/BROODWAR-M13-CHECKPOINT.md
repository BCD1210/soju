# M13 live pacing checkpoint

Private evidence is in `~/.local/share/soju/broodwar-native/3d-proof/m13/` (`runtime/3d-proof/m13/`). No game assets are tracked. Work remains local on `feat/broodwar-3d`. **Pacing acceptance is not achieved, and frame-critical isolation is incomplete.**

## Rebuilt baseline

Rebuilt `2683056`, including the previously unmeasured `f0a6dd1` typed frame-catalog cache. `baseline/live-launch.json` records binary SHA256 `c0867d7bdde21766793746e5d0e3327d41c49d24ce6f0313bee5d85d3b244b1f` and LaunchServices nice 0. The private land duel advances with the existing economy/defense controller and a scouting order.

The 100.400-second short run measured cadence **14.77 ms p95**, stereo CPU **5.11 ms p95**, stereo GPU **9.94 ms p95**, and **615 estimated missed refresh slots**. Instrumentation overlaps this window; it is diagnostic, not a ten-minute acceptance run or a controlled comparison with M12's later, larger army.

`baseline/time-profiler.trace`, exported signposts/profile XML, `final-signposts-summary.json`, `final-main-profile.txt`, and `sample.txt` retain the investigation. The initial combined instrument capture stalled and was interrupted; its log and partial trace remain. The subsequent standard Time Profiler template includes Points of Interest and exported successfully.

| Signpost | M12 post-run p95 ms | Rebuilt short-run p95 ms | Rebuilt maximum ms |
| --- | ---: | ---: | ---: |
| Billboard update | 20.32 | 0.773 | 1.641 |
| Batch prepare | 17.21 | 0.004 | 74.613 |
| State apply | 32.35 | 30.847 | 42.389 |

The catalog-cache build no longer shows the large billboard tail in this small-population window. State application still blocks the main thread. Time Profiler attributes 965 ms of sampled main-thread time to state application and 972 ms to overlay rendering. These observations do not prove a large-population speedup.

## Implemented changes

- `66a56f7`: native unit dictionary construction and dirty comparisons run on the bridge queue. Unchanged fallback units skip reapplication. Fog/creep updates visit changed tiles and adjacent tiles; region changes still refresh the region. Fog masks have a separate revision from terrain geometry. Mask-only changes retain terrain vertices and diffuse arrays and publish new fog pages with changed slices, retaining old pages for submitted work.
- `66a56f7`: Metal batch construction runs on a serial preparation queue with one outstanding request. The render side retains the previous complete snapshot until publication. Prepared transforms, opacity, visibility and billboard flags replace per-instance SceneKit reads in the Metal draw loop. GPU instance storage remains triple-buffered. Detached geometry/material ownership prevents the worker from reading live mutable scene nodes. Old batches retire on the worker queue.
- `c353753`: unchanged sprite snapshot geometry/materials are reused. Live projectile image decoding and Metal texture upload run on a bounded worker cache; pending frames retain the previous displayed frame. Repeated assignments of unchanged effect materials are suppressed.
- `f69627e`: GPU visibility completion callbacks retain their submitted batch count rather than reading the array being published. Frames before initial publication explicitly clear the render targets. An early measurement was stopped for this audit fix and is retained as `publication-audit-abort`; it is not accepted evidence.
- `2239bdb`: relief materials now have a 64 MiB cache budget charged for retained Metal maps and the eventual upload size of image-backed maps. The previous 2,048-entry cache charged one unit per material, allowing map ownership to outlive the separate texture cache's budget. The global 1,536 MiB texture cap is unchanged.
- `3ebc03b`: the material-only budget was insufficient (`material-budget-gap/` preserves the interrupted attempt). Live preparation cache ceilings now total 704 MiB: stream frames 192, relief data 96, relief materials 32, terrain source textures 128, packed native groups 96, batch image textures 128, and effect textures 32. This leaves room under the unchanged global cap for active scene/snapshot ownership and render targets. Shared textures may be conservatively charged by more than one cache; these ceilings are not a total-process residency estimate.
- `c9e9157`: streamed effect textures preserve the original image orientation. The real SceneKit color probe in `effect-origin.swift` / `effect-origin.txt` renders an asymmetric two-color image and confirms that `topLeft` matches the NSImage path, while `bottomLeft` flips it.

The first completed M13 duel is retained in `residency-gap/`: 610.099 seconds, 12.87 ms cadence p95, 7.81 ms CPU p95, 9.67 ms GPU p95 and 1,165 missed slots. Frames advanced from 1,309 to 15,345, owned units remained present (5–40), and an opponent was observed. However, it reached the texture cap and recorded 9,587 rejected allocations, so its timings cannot certify complete visual output. The material-budget fix was made afterward and requires its own full run.

`incremental/` and `effects/` retain the intermediate profiles. Their `thread-signposts.json` records show batch preparation and state diff intervals on worker threads, with no main-thread batch preparation intervals. In the effects-build profile, batch preparation is 1.815 ms p95, billboard update 0.704 ms, snapshot capture 2.371 ms, and state application 19.978 ms. State application still reaches a 390.691 ms maximum during this window. These traces include different game moments and instrumentation; they are not controlled speedup ratios.

| Short run | Seconds | Cadence p95 ms | Stereo CPU p95 ms | Stereo GPU p95 ms | Missed slots |
| --- | ---: | ---: | ---: | ---: | ---: |
| Rebuilt baseline | 100.400 | 14.77 | 5.11 | 9.94 | 615 |
| Worker and dirty terrain | 100.162 | 13.91 | 9.05 | 8.33 | 365 |
| Material reuse and effect streaming | 62.775 | 13.08 | 5.34 | 10.12 | 194 |

The metrics reporter samples `malloc_zone_statistics` on its utility queue. `mallocZoneStatistics` includes blocks in use, bytes in use, allocated bytes and peak bytes. This working fallback measures retained allocations across all malloc zones; it does **not** count transient allocation traffic or establish zero allocations per frame. The final run additionally retains a periodic allocation timeline.

## Remaining architecture gaps

Full frame-critical isolation is not implemented. Main-thread scene application, billboard orientation/geometry updates, detached snapshot capture, SceneKit overlay rendering and visibility-buffer publication still perform work and allocate. New terrain geometry and fog-mask image construction still run on the main thread; fog topology changes still rebuild terrain arrays on the worker. Typed draw instances remove one bridging path, not every Objective-C/Swift bridge in the frame loop. Streaming lookup/publication uses short cache locks, and rendering has short visibility/metrics locks and a GPU ring semaphore; no mutex is held for an entire frame.

The next boundary is a worker-owned scene description, including terrain, billboards and effects, with immutable typed render packets and a separate UI/selection representation. Moving the existing `Commander.apply` closure to a background queue would race camera/input, selection, diagnostics and SceneKit rendering on the same mutable nodes. That unsafe relocation was not used.

## Verification

Python: **282 passed and 60 subtests passed**. Rust: **14 passed**. Final build: **125 runtime checks passed** each for desktop, XR, and XR SceneKit fallback. New checks cover captured-transform isolation, completed asynchronous publication, preserved terrain buffers across fog changes, immutable fog replacement, and dirty-state/session behavior.

The complete Python rerun initially exposed a standalone wire-decoder compilation failure because `PreparedState` had been placed inside the source range extracted by that test. `8fab9b0` moves state preparation into `CommanderWorld.swift`; the transport test was not weakened or changed. The full suite then passed with the counts above.

## Final ten-minute private duel

The measured build is `8fab9b0`, binary SHA256 `6e548bb1aae218eb4c6b0a77c3acb28b1fa0e296fed0a586d6dc35f35c0cf2c2`. LaunchServices nice **0** is recorded in `final/live-launch.json`. No profiler or builds overlapped the final ten-minute measurement. The same private land scenario was used with economy/defense orders and a separate scout, recorded in the private controller and order files.

| Measurement | M12 final | M13 final |
| --- | ---: | ---: |
| Duration, seconds | 609.985 | 610.101 |
| Cadence p95, ms | 22.47 | 13.01 |
| Stereo CPU p95, ms | 17.87 | 6.68 |
| Stereo GPU p95, ms | 9.47 | 8.68 |
| Estimated missed slots | 6,625 | 1,191 |
| Missed slots per ten seconds | 108.61 | 19.52 |
| Maximum native-visible population | 65 | 50 |

Native frames advanced **1,244 → 15,645**. Owned units remained present in every sample, ranging **5–40**, and the opponent was observed. These satisfy the duration and gameplay conditions. The runs have different population histories, so the table is not a controlled speedup ratio. CPU and GPU overlap; missed slots are cadence estimates rather than compositor telemetry.

Both pacing gates **fail**: 13.01 ms exceeds 11.1 ms, and 19.52 missed slots per ten seconds exceeds the required rate below one. The final renderer texture peak was **1,220.64 MiB**, with **zero rejected allocations** under the unchanged 1,536 MiB cap. `final/allocation-samples.json` retains 64 malloc-zone samples: 237,036–7,165,233 blocks in use and 110,163,440–297,260,128 bytes in use. These process-wide retained counts cannot prove zero transient frame allocations. Command buffers, completion closures, snapshot capture, visibility publication and SceneKit work still allocate.

## Final-build trace attribution

`postfinal/time-profiler.trace` and its exported XML retain a separate 25-second late-game diagnostic on the same final build. It is outside the accepted duration window.

| Signpost | Thread | p95 ms | Maximum ms |
| --- | --- | ---: | ---: |
| State application | Main | 16.599 | 18.069 |
| Snapshot capture | Main | 2.964 | 8.367 |
| Billboard update | Main | 1.001 | 5.612 |
| Scene render/encode | Main, per eye | 3.030 | 9.627 |
| Batch preparation | Worker | 12.794 | 49.899 |
| State diff | Worker | 2.821 | 3.349 |
| Fog slices | Worker | 1.852 | 1.852 |
| Runtime wait | Main | 11.377 | 17.402 |
| GPU ring wait | Main | 0.001 | 0.014 |

State application still occupies the same main queue that drives XR frames, exceeding a refresh interval by itself. Snapshot capture and SceneKit transaction/overlay work add to that contention. Time Profiler records 1,420 ms inclusive in `C3DTransactionFlush` and 1,369 ms in `MetalBatches.prepareAsync` during this diagnostic. GPU ring waits are negligible, so this trace does not support blaming a saturated instance-buffer ring. Worker preparation is no longer in the eye callback, but the main-thread ownership boundary described above remains necessary to meet the target.

Regenerate the private table and explicit acceptance record with `python3 runtime/3d-proof/m13/summarize_m13.py`, using the runtime path above. The acceptance record keeps pacing, full isolation and zero-allocation gates false.

## Native population stress

`Soju-Population.scx` adds 120 owned Marines to the same private terrain; its provenance and generation inputs remain outside git. This is a separate stress scenario, not a replacement for the ten-minute duel. The initial camera-limited attempt is retained in `population-camera-gap/` and excluded from acceptance.

The measured `population/` run uses nice 0 and a camera covering the army. It retains **125 owned units**, **138–139 native-visible objects**, and advancing frames **2,984 → 4,666**. Groups of Marines receive movement orders during the recording. `view-diagnostics.json` records **278 visible instances and zero culled instances**; `window.png` shows the rendered army. Its 75.083-second instrumented window measured **24.85 ms cadence p95**, **22.51 ms CPU p95**, **5.97 ms GPU p95**, and **934 missed slots**. Texture peak was **1,255.86 MiB**, with zero rejected allocations.

The population trace measures main-thread state application at **25.106 ms p95 / 89.910 ms maximum**, snapshot capture at **11.205 / 17.705 ms**, billboard updates at **3.674 / 6.597 ms**, and per-eye render encoding at **6.411 / 24.614 ms**. Worker batch preparation is **15.734 / 140.633 ms**; GPU ring wait remains **0.001 / 0.020 ms**. Time Profiler attributes 3,993 ms inclusive to `C3DTransactionFlush`, 2,308 ms to state application, 2,064 ms to snapshot capture, and 1,826 ms to `SCNMaterial.copyWithZone`. Inclusive weights overlap and must not be summed.

This directly demonstrates the remaining population-dependent main-thread cost. Material/node copying and SceneKit transaction/finalization work remain in the critical path; moving GPU buffer packing alone does not bound it below a refresh interval. `postfinal/thread-signposts.json` and `population/thread-signposts.json` verify that batch preparation, state diffing and fog-slice uploads are on workers, while state application and snapshot capture remain on the main thread. Full worker ownership of typed scene/render state and removal of per-frame allocation paths are still required.

## Handoff

The original private land duel and its terrain selection were restored after stress testing (`restore-duel.log`, `restore-terrain.log`). Measurement viewers have exited. Dots's `asset-upgrades/` was untouched; no game assets were staged. No D2R process or shared runtime service was stopped. All commits are local; nothing was pushed. The pre-existing untracked `tools/broodwar_native/target/` build directory remains.
