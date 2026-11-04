# M10 stereo rendering checkpoint

The frozen 1,728-object replay now fits the 11.1 ms **GPU** p95 budget, but end-to-end frame cadence still fails it. This is an opt-in prototype, not a completed 90 Hz VR renderer. Default SceneKit rendering remains available.

Private evidence: `~/.local/share/soju/broodwar-native/3d-proof/m10/` (`runtime/3d-proof/m10/`). No game assets, snapshots, binaries or captures are tracked. Nothing was pushed. `asset-upgrades/` was not modified; only Brood War viewers were restarted. No D2R process was stopped and the shared runtime configuration was unchanged.

## Changes and commits

- `043eed4`: stereo stage measurements and signposts.
- `e84d60c`: bounded decoded sprite/atlas/relief caches and shared material textures.
- `2843d33`: opt-in instanced Metal batches, terrain texture arrays, conservative frustum culling, stationary-view occlusion queries, amplified layered stereo, relief LOD and shadow batching.
- `b2181cd`: native plane texture-origin correction, including GPU readback validation.
- `c694133`: building relief anchoring to opaque mesh support samples.
- `ac4d994`: single-sample stereo output and faster timer polling, leaving pacing to the runtime.

`--metal-batches` requires `--snapshot` and sprite-backed objects; gameplay wrist panels and rigged models are unsupported. It uses both runtime eye poses/FOVs in one vertex-amplified array pass and copies the layers to the two swapchains. Instance buffers are triple buffered and reused only after command completion. Terrain has 128-slice texture-array pages; HD sprites still bind separate material textures. Relief becomes flat below a 48-pixel projected bound. Occlusion results are reused only for unchanged stereo matrices, invalidated for source revisions and periodically refreshed.

## Measured performance

Apple M4 Pro, 48 GiB; x86_64 viewer; 1512×1680 per eye, 90 Hz requested, hardware HEVC simulator stream at 2272×1264. Frozen `m9/eight-player.json`: eight players × (200 army units + 16 buildings). Normal visibility culling applies. Both-eye CPU encoding and summed command-buffer GPU execution overlap; these durations must not be added. Frame cadence measures consecutive left-eye starts, including runtime pacing, not network receive latency.

| Run | Frame mean / p95 ms | CPU mean / p95 ms | GPU mean / p95 ms |
| --- | ---: | ---: | ---: |
| SceneKit baseline | 71.526 / 108.059 | 55.651 / 58.720 | 26.907 / 30.866 |
| Two-eye batching, capture tooling | 21.312 / 35.397 | 11.273 / 20.287 | 35.064 / 47.003 |
| Amplified stereo, 4× samples, LOD | 13.361 / 23.304 | 3.430 / 4.057 | 13.435 / 16.097 |
| Specialized shaders, occlusion, 2× samples | 12.493 / 22.532 | 3.242 / 3.891 | 10.421 / 12.409 |
| Final single-sample runtime-paced replay | 11.954 / 22.545 | 3.536 / 6.506 | 8.571 / 10.583 |

Final saved measurement: 3,870 completed stereo GPU samples, simulator connected, capture disabled, warm-up excluded. Ordinary background applications remained running. Intermediate results include different instrumentation and connection conditions and are diagnostic, not controlled isolated attribution of each optimization. The final live viewer continues running; the report uses the immutable `final-snapshot-metrics.json`.

Per-eye CPU encoding fell from 27.259 mean / 30.129 p95 ms to 1.177 / 2.656. Final eye setup is 0.545 / 1.255; post-render 0.004 / 0.008; per-stereo billboard update 0.208 / 0.370. Full tables are in `timings.md`.

`batched.gputrace` and `amplified-stereo.gputrace` are actual Metal captures. Signposts and structured stage measurements accompany `startup-sample.txt` and `render-sample.txt`; SceneKit draw submission dominated baseline CPU time. The attempted Instruments Time Profiler recording hung and is not valid evidence. Final measurements exclude capture overhead. The single-sample tradeoff reduces antialiasing; it does not establish visual parity at a headset.

## Visual and tracking evidence

`xr-four-angles.png` summarizes same-frame stereo swapchain pairs from `final-ne/`, `final-nw/`, `final-sw/`, `final-se/`, frame 15. These captures use **2× MSAA**, while final performance uses **1×**; `acceptance.json` records their different binary hashes. Final 1× output was also inspected in the running simulator. Texture origins, team paint and solid relief are retained. Cliff stretching remains visible.

Buildings are offset using the lowest opaque support sampled on the actual relief grid. This improves grounding without changing canonical projection, but does not guarantee a flush full footprint on sloped terrain or reconstruct hidden sides. All-building contact acceptance remains open.

`head-translation.json` verifies native simulator scroll movement through the tracked runtime pose: 0.699997 m head translation, matching the viewer world transform to 0.000000568 world units; presentation and desired cameras match. No viewer pose was injected. This is simulator tracking evidence, not a physical-headset test or a motion-performance pass.

Canonical SceneKit comparison (`canonical-final/frame-00003-scores.json`) remains unchanged: regional SSIM **0.857816, 0.886722, 0.869301, 0.867028, 0.890707, 0.898492**; maximum crop-origin alignment error **0.500015 px**. All six regions pass 0.85 and alignment passes 1 px. The reference remains the independent HD compositor, not a Remastered framebuffer. This gate covers the retained canonical route, not a separate certification of the experimental Metal renderer.

## Verification and open gates

- Asset-venv Python suite: **277 passed, 60 subtests passed** (`tests-final.log`).
- Rust suite: **14 passed** (`tests-rust.log`).
- Optimized desktop and XR builds; **95 runtime checks each**, including specialized amplified pipelines, texture-origin readback, actual plane UVs, cache eviction and GPU callback pairing.
- GPU p95: passes. End-to-end frame p95: fails; approximately 22.5 ms spikes persist despite runtime-based pacing.
- HD frame arrays and asynchronous frame streaming: incomplete. Camera-direction changes synchronously load/rebuild and can stall for tens of seconds.
- Hard GPU residency limits: incomplete. Byte-limited caches can evict ownership, but active scene nodes/batches pin resources.
- Zero per-frame CPU allocations: incomplete; pooled instance buffers and bounded metrics do not remove SceneKit/HUD/source-update allocations.
- Cliff stretching and complete building contact acceptance: incomplete.

The next work must trace runtime wait/encode/present scheduling, move directional-frame preparation off the render path, implement HD frame residency/arrays, and validate moving-view cadence before enabling this renderer for gameplay. Do not interpret the stationary GPU pass as VR readiness.

## Reproduction

Build with the asset-venv Python: `python tools/broodwar_native/build_scene.py --xr`. The build signs the output before replacement. `final-launch.json` records the exact viewer invocation and binary hash. Use the private M9 XR manifest; do not change shared runtime settings. Connect the existing simulator before creating the file passed to `--metrics-start-file`. Do not enable PNG recording or Metal capture during timing. Run `python tools/broodwar_native/report_m10.py` in asset-venv to regenerate the private timing table, contact sheet and acceptance report from saved evidence.
