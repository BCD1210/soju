# M14 worker-owned scene and frame-resource checkpoint

**Status:** Scene ownership and frame-resource changes are implemented and the corrected final live runs completed. Both runs recorded zero estimated missed slots, but cadence p95 remains above 11.1 ms and literal zero transient allocations is unproved.

Private evidence: `~/.local/share/soju/broodwar-native/3d-proof/m14/`. Code is committed locally; nothing was pushed. Game assets, captures, and `tools/broodwar_native/target/` are not tracked. `asset-upgrades/` was untouched. Only the dedicated native bottle and its viewers were restarted; no D2R or shared runtime process was stopped.

## Implemented boundary

The default desktop and XR Metal callbacks consume a complete scene packet and scalar camera values. They no longer traverse or mutate SceneKit nodes, capture a scene snapshot, update unit billboards, or invoke `SCNRenderer`. XR wait/acquire/encode/submit runs on a dedicated user-interactive thread, independent of main-thread UI and native-state delivery.

`NativeSceneWorker` owns native visual state application, terrain, fog, creep, unit image chains, world sprites, effects, selection rings, health bars and overlay preparation. Native updates remain ordered with bridge backpressure; only view updates coalesce. Resource readiness republishes a packet even without a new simulation update. Session changes reject obsolete views and publications. Terrain failures retain the previous complete packet, report the error, and retry.

Published `ScenePacket` and `PacketBatch` types contain scalar draw records and retained GPU resources, without construction nodes. One pending publication, a displayed packet and retirement ownership protect CPU payloads; three GPU flight slots retain submitted batches and prevent instance/query-buffer overwrite. Builder-private SceneKit geometry remains a construction adapter on the worker. SceneKit rendering is used only by the explicit fallback (`SOJU_BW_RENDERER=scenekit`). Current native image-chain content is supported in Metal; authored rig animation remains a fallback capability. The inspected private catalog had no fidelity-approved unit rigs and no authored effect models, so this does not establish Metal skinning parity for future rigs.

Main-owned camera, UI and selection publish independent values and immutable geometry/image inputs. Desktop and XR picking use packet triangle geometry; desktop projection uses the submitted camera and viewport. Input coalescing preserves trigger, grip and snap-turn transitions. Exceptional event-queue exhaustion reports an error rather than silently losing commands. Normal pose-only stalls do not fill the queue.

Terrain geometry and fog images are constructed on the worker. Fog changes dirty only affected tiles and neighboring shared corners. Fixed 16×8 terrain pages retain unaffected geometry and diffuse arrays; mask-only changes retain the prior submitted resources and replace mask ownership. No main-thread terrain installation occurs on the default path.

## Frame resources and visual correction

Sixteen precreated metric completion slots and three capture slots replace per-eye completion closures and recurring mirror texture creation. A 125-unit check exposed a late first use of the second capture texture; startup now prepares all three textures before leasing any slot, and the final acceptance runs repeat on that corrected build. Capture slots remain leased through GPU copy and background image conversion. A persistent dispatch source handles conversion and capture metadata. Mirror publication rejects older completed frames; conversion failures and capture-save failures are reported separately. Visibility storage and flight completion handlers are preallocated. No frame-loop scene dictionary conversion or SceneKit bridge is required; raw Metal API interop remains.

Live desktop QA found that SceneKit primitive sources are normalized independently of their declared dimensions. Explicit plane, sphere and torus extraction now preserves health-bar aspect ratio, projectile radius, and ring/tube radii. GPU readback verifies 114×8-pixel desktop and 116×8-pixel XR bars. Desktop box selection selected 12 Marines; a right-click move received a native acknowledgement for all 12 and visibly moved them. Corrected overlay screenshots are under `desktop-final/`; the original interaction evidence is under `desktop-qa/`.

## Timing and allocation measurement limits

`frameOwnerCreationRequests` reports instrumented creation requests from the XR frame owner, separately for measured steady state and warm-up. It includes Metal command-buffer/render/blit encoder requests. It is not a malloc interceptor. `frameAllocationDelta` samples all-zone blocks-in-use around each frame; concurrent workers contribute and frees can cancel allocations. Neither counter proves literal zero transient allocations.

The separate Allocations attachment attempt remained at “Starting recording / Attaching” beyond its requested 25 seconds and beyond viewer exit. It produced no usable allocation-event trace. Its own stuck profiler process was terminated after normal interrupt/termination signals did not work. Details are retained in `allocation-trace/trace-status.json`. Zero transient allocations therefore remains unproved, and framework object requests remain nonzero.

Runtime timing measures `xrWaitFrame`, acquisition, `xrEndFrame`, and total work after wait returns. The latter is compared with `predictedDisplayPeriod`; it is not a physical presentation deadline. The inspected runtime source uses absolute-grid `sleep_until`, then input/tracking work, and returns elapsed session wake time as `predictedDisplayTime`. Source-to-private-binary identity was not certified. No shared runtime code or service was modified. Runtime library identity and presentation limitations are recorded in `pacing-runtime-provenance.json`.

## Diagnostic comparison

The first unprofiled worker population check (`worker-short-ready/`) retained 125 owned units and 135–138 visible units for 75.09 seconds: cadence p95 12.91 ms, CPU p95 0.55 ms, GPU p95 5.56 ms, and 1.065 missed slots per 10 seconds. This was a pre-final diagnostic, not final acceptance.

A separate 25-second Time Profiler capture (`worker-thread-trace/`) showed main state-application p95 **0.541 ms**, versus **25.106 ms** in the earlier population trace. World tick p95 was **0.108 ms**. No SceneKit renderer, billboard-update or snapshot-capture calls were sampled on the XR frame stack. Sampling is supporting evidence rather than exhaustive call counting; the default callback code boundary supplies the structural guarantee.

The same trace measured runtime wait p95 **12.272 ms**, full frame-loop p95 **12.873 ms**, GPU ring wait p95 **0.0023 ms**, and runtime submission p95 **0.010 ms**. Batch preparation (**15.677 ms p95**) and terrain construction (**25.288 ms p95**) remain background work. They can age packets but no longer block the frame loop. The remaining cadence boundary is runtime wait/wake timing, not the former main-thread scene work.

## Long-run geometry failure and ownership correction

The first full-duel attempt on `bedfc55` exited with `EXC_BAD_ACCESS` after its last recorded 590.10 seconds. It is **not** a completed acceptance run. The crash report, last metrics and last published 50-unit snapshot are retained in `final-duel-prewarmed/`. The crash was on `soju.bw.scene-owner` in `C3DMeshGetElementAtIndex` / `SCNGeometry.geometryElements`, called by `MetalBatches.mesh`. Binary disassembly identified the native billboard group extraction call site, rather than terrain or overlays. `FrameStream` still published shallow-copied SceneKit geometry from its decoder queue; this left a construction-object ownership boundary to correct. The crash record does not identify the dynamic geometry type or prove SceneKit’s internal failure mechanism.

`fbdcbd5` replaces decoder-side SceneKit geometry and material publication with immutable vertex/UV/index arrays and typed texture/color/shader values. Every consumer constructs fresh SceneKit sources, elements and material properties. Tests verify actual streamed geometry/material isolation and 1,024 concurrent reconstruction, mutation and cache-eviction cases. This removes demonstrable cross-queue backend sharing; successful replay and a full repeat supply the incident validation, not a claim of proof from the stack alone.

## Final verification and live acceptance

Final code: `fbdcbd5`. Python: **282 passed plus 60 subtests**; Rust: **14 passed**. Exact final builds pass **172 desktop, 177 XR, and 172 explicit fallback runtime checks**. Source and binary hash manifests are in `worker/mesh-final-source.sha256` and `worker/mesh-final-binaries.sha256`. The focused code reviews are clear. Runtime checks cover packet/session ownership, event deduplication, terrain failure/recovery, picking, input coalescing, GPU output, primitive dimensions, slot exhaustion/reuse and monotonic mirror publication.

The final population run (`final-population-owned-stream/`) lasted **75.110 seconds** at nice 0, without a profiler or concurrent build. It retained **125 owned units**, **135–141 visible units**, and advanced native frames **977→2665**. Cadence p95 was **12.68 ms**, CPU p95 **0.49 ms**, GPU p95 **6.38 ms**. There were **zero estimated missed slots**: the missed-slot gate passes, but the **<=11.1 ms cadence gate fails**. After-wait work was **0.67 ms p95**, maximum **5.314 ms**, with no period overruns; runtime wait was **12.07 ms p95**.

Its measured creation counters were zero for capture/render textures, texture descriptors/views and render-target rebuilds. Startup prepared all three capture textures. The 6,761 measured frames still requested 13,522 Metal command buffers, 6,761 render encoders and 13,670 blit encoders. These are object request counts, not transient allocation-event counts. One sampled worker packet age reached **135.9 ms**; packet preparation latency remains distinct from the short encoding time.

The saved pre-crash state survived a separate **60-second frozen-snapshot replay** (`crash-snapshot-replay/`). This is a reproduction check, not live acceptance. The corrected **610.100-second live duel completed** (`final-duel-owned-stream/`) at nice 0 without profiling or concurrent builds, with strictly advancing native frames **1020→15452** and visible opponent activity. It had **5–13 owned units** and **12–29 visible units**; combat eliminated the workers and command center, so its later portion was a lighter workload. The existing benchmark controller was unchanged. The separate 125-owned-unit run supplies the high-population acceptance evidence.

Final duel cadence p95 was **12.71 ms**, CPU p95 **0.33 ms**, GPU p95 **5.33 ms**, with **zero estimated missed slots**. Runtime wait p95 was **12.29 ms**; after-wait work was **0.46 ms p95**, maximum **2.200 ms**, with **zero period overruns**. All measured application resource-creation counters were zero. Its 54,910 measured frames still requested 109,820 Metal command buffers, 54,910 render encoders and 111,025 blit encoders. Renderer-owned texture residency peaked at **741.42 MiB**, with no rejections.

Both final runs use XR binary SHA-256 `2a278ddd11e9d24c153a968d1cd906b56df8a60ed97f78a2e6ea5f5dbabe419d`. The 60-second replay, final stress and full duel completed after the ownership correction. The failed earlier attempt remains explicitly incomplete; one successful long repeat is evidence of recovery, not proof that every possible lifetime failure is eliminated.

| Workload | Cadence p95 ms | CPU p95 ms | GPU p95 ms | Estimated missed slots / 10 s |
|---|---:|---:|---:|---:|
| M13 final duel (reported reference) | 13.01 | 6.68 | 8.68 | 19.52 |
| M14 final duel, 610.10 s | **12.71** | **0.33** | **5.33** | **0.00** |
| M14 pre-boundary 125-owned-unit reference (`population/`) | 23.32 | 19.60 | 6.44 | 94.00 |
| M14 final 125-owned-unit stress, 75.11 s | **12.68** | **0.49** | **6.38** | **0.00** |

Duel trajectories and unit populations vary; the fixed 125-owned-unit stress is the stronger like-for-like population comparison. The reference population trace was separate from its unprofiled timing run. Percentiles retain the same 0.01 ms upper histogram edge convention; thresholds were not relaxed.

## Remaining gates

- **Cadence p95 <=11.1 ms: FAIL**, for both final runs. Dedicated encoding fits within the reported refresh period, but runtime wait/wake jitter remains. The next pacing boundary is the runtime/compositor’s wait deadline and physical presentation telemetry. This work did not modify or restart shared runtime services.
- **Estimated missed slots <1 per 10 seconds: PASS**, for both final runs. This metric estimates missed nominal slots from frame intervals; it is not physical display telemetry.
- **Literal zero transient allocations per frame: UNPROVED / NOT ACCEPTED.** Application resource-creation counters are zero after warm-up; Metal command-buffer/encoder requests remain nonzero, and the allocation-event recorder did not produce usable data. Net malloc deltas cannot close this gate.
- Worker packet preparation can still lag simulation/view updates; the sampled 135.9 ms packet age is retained rather than hidden by the short frame encoding duration. Authored rig rendering remains an explicit fallback capability.

M14 therefore completes the scene-ownership and frame-resource changes with passing regression suites and final live runs, but it **does not pass all requested acceptance gates**. Machine-readable results and limits are in private `acceptance.json`.


## Local commits

- `175eb26`: scalar camera packet boundary.
- `958f6cf`: frame-local net malloc and runtime submission timing.
- `f6a6a86`: immutable worker terrain and incremental spatial pages.
- `b0c36b4`: worker-owned full scene packets and independent Metal frame owner.
- `00e9f84`: reusable completion/capture slots, creation counters and correct primitive dimensions.
- `bedfc55`: prewarm every capture texture before admitting the first in-flight capture.
- `fbdcbd5`: replace streamed SceneKit construction objects with immutable value payloads and owner-local reconstruction.
