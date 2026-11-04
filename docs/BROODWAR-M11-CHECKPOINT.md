# M11 default renderer checkpoint

Metal batching is now the default for desktop and XR. This checkpoint does **not** meet the complete live 90 Hz or geometry acceptance requirements.

Private captures, launch records, test logs and timing tables are under `~/.local/share/soju/broodwar-native/3d-proof/m11/` (`runtime/3d-proof/m11/`). No game assets are tracked. No push was performed. `asset-upgrades/` was not modified; no D2R process or shared runtime service was stopped.

## Default and fallback

Normal desktop and XR launches select Metal batches without `--metal-batches`. To select the retained SceneKit renderer, set `SOJU_BW_RENDERER=scenekit` in the viewer or launcher environment:

```sh
SOJU_BW_RENDERER=scenekit scripts/soju bw-native scene
SOJU_BW_RENDERER=scenekit scripts/soju bw-vr
```

Desktop uses 4× antialiasing; XR uses one sample. Desktop captures and F12 read the Metal output. Rigged models, combat effects, reticles and wrist controls use a separate transparent SceneKit pass sharing batch depth. Hit-testing includes batched geometry; inherited cloak opacity is retained. Source changes invalidate batches, including terrain membership and fog-mask changes.

Color uploads explicitly convert profiles to sRGB and use sRGB texture views. MetalKit's load option alone returned unorm formats on this machine. Normal and depth maps remain linear. The orthographic shader uses the reference light and view direction. These fixes were needed to preserve the canonical gate on the default renderer.

## Streaming and pacing

A serial background queue prepares HD images, team colors, relief geometry and GPU textures. Requests are deduplicated, limited to 64 pending entries, and adjacent directional frames are prefetched. The render path retains the previously displayed frame until the replacement is complete. Frame identity includes image, animation frame and team color. A completed-frame LRU cache has a 384 MiB budget; oversized entries are not repeatedly queued.

That budget is **not** a total residency guarantee: decoder caches and active scene/batch references retain additional memory. Independent map sprites and terrain preparation still have synchronous work. Batch rebuilding for native updates also remains on the main thread.

Active XR sessions advance directly through `xrWaitFrame`; an inactive session uses a short retry delay. There is no second periodic 90 Hz timer. Render-pass descriptors, depth views and swapchain color views are reused. Unchanged native image chains reuse their geometry, per-frame evidence creation is reduced, and metric storage is fixed-size. SceneKit, command submission, native updates and direction changes still allocate; zero allocations has not been established.

The timing collector no longer stops at 10,000 samples. Fixed histograms retain arbitrarily long sessions, report upper percentile edges at 0.01 ms resolution, and keep long stalls in the maximum/overflow statistics. Report serialization runs on a utility queue, outside GPU completion callbacks. Arm `--metrics-start-file` after warm-up; there is no implicit removal of initial samples. Estimated missed refresh slots are separate from small nominal-budget overruns and are not compositor telemetry.

## Evidence and remaining gates

The final default canonical render has regional SSIM **0.858234, 0.886899, 0.869327, 0.863165, 0.888381, 0.898485**, with maximum alignment error **0.500015 px**. All six regions pass 0.85 and alignment passes 1 px. Exact results and capture paths are recorded in private `acceptance.json`. The reference is the independent HD compositor, not a Remastered framebuffer.

`xr-four-angles.png` shows same-frame left/right swapchain captures from four positions. It uses the 34-object frozen scene, not every building type. Cliff stretching remains visible and complete footprint contact on sloped ground remains unresolved; the existing minimum-support grounding convention is retained.

The dedicated game launch timed out before opening the native bridge port. The loader remained in macOS process state `U`; the bottle-scoped stop command failed, and a previously running viewer also remained uninterruptible. Shared services were left alone. Consequently, **no ten-minute live 1v1 measurement was obtained**, and live cadence p95 ≤11.1 ms is unproven. The separate capture-free frozen-scene diagnostic in `timings.md` must not be used as a substitute or compared directly with M10's 1,728-object scene.

Final capture-free diagnostic (34-object snapshot, 30-second warm-up, **64.987 seconds** measured):

| Measurement | Mean ms | p95 ms | p99 ms |
| --- | ---: | ---: | ---: |
| Frame cadence | 11.147 | **15.410** | 18.530 |
| Stereo CPU submission | 1.998 | 5.020 | 7.540 |
| Stereo GPU | 3.384 | 4.560 | 6.180 |

There were **5,830 cadence samples** and **103 estimated missed refresh slots**. Even this frozen diagnostic fails the 11.1 ms cadence target. CPU and GPU overlap and must not be added. Other user applications remained running.

Rebuild and verify with asset-venv Python. Run `tools/broodwar_native/report_m11.py` to regenerate the private table, contact sheet and acceptance record. The report explicitly keeps overall acceptance false until the remaining live, geometry, allocation and residency gates pass.

## Verification and commits

- Asset-venv Python: **277 passed, 60 subtests passed** (`tests-final.log`).
- Rust: **14 passed** (`tests-rust.log`).
- Final optimized desktop and XR builds: **107 runtime checks each**. Explicit SceneKit fallback: **107 checks**.
- Checks include background publication, first-frame visibility, same-frame team changes, inherited opacity, selection after actual rendering, sRGB formats, texture origin and long-session metric overflow.
- `276b5ae`: fixed-storage long-session cadence metrics.
- `f29416c`: default desktop/XR batches, streaming, color parity, control preservation and runtime-paced submission.

Launch records retain binary hashes. The canonical capture and final diagnostic use the final builds. Four-angle images were captured before the last resource-reuse and stream-identity correctness changes; they are visual evidence, not certification of all building types or a final-build live motion test.
