# M9 relief and XR checkpoint

M9 adds depth-displaced HD sprite surfaces, corrects black team paint, preserves the canonical comparison gates, and records four low-angle stereo views. It does **not** close ground-level visual acceptance: cliff stretching and single-view building reconstruction remain visible limitations.

Private evidence root: `~/.local/share/soju/broodwar-native/3d-proof/m9/` (the native runtime's `runtime/3d-proof/m9/`). No owned textures, captures, runtime binaries or scene snapshots are tracked.

## Changes

- `31cc680`: reject uninitialized zero player-color slots. The captured native state has eight `[0,0,0,0]` slots; multiplying those into the team mask made the Marine's painted armor black. Sprites now use the existing player palette fallback, matching the model path. The independent reference compositor's identical bug was corrected explicitly in that commit.
- `d7484e6`: export HD material layers, create tessellated depth relief, apply per-pixel material response, record both XR eyes, and measure stereo CPU/GPU/frame times. Includes a labeled eight-player rendering fixture and per-type source audit.
- `522569c`: subdivide VF4 cliff transitions with continuous profiles and increase anisotropic terrain filtering to 16. Original HD cliff-tile UVs are retained.
- `2c2e85e`: remove the 2D sprite draw-order bucket from relief geometry. The northeast view exposed invisible depth-tested sprites; removing terrain or lighting did not fix it, whereas ordinary geometry ordering did. Shadows now read scene depth. Four direction checks protect this behavior.

- `af753d4`: pair both GPU completions regardless of callback order; the heavy scene exposed right-eye-first completion. Includes a regression for both orders.

## Installed layers and rendering

`installed-layers.json` records hashes and layer names for all 858 source ANIMs resolved by the installed world-sprite catalog:

| Layer | Sources |
| --- | ---: |
| diffuse | 858 |
| bright | 217 |
| teamcolor | 153 |
| normal | 216 |
| specular | 216 |
| ao_depth | 217 |
| emissive | 86 |

Normal R/A decode to signed X/Y with positive reconstructed Z. AO/depth G/A decode to occlusion/depth; diffuse alpha alone determines coverage. Normals and AO/depth reach the shader through linear UNORM data textures. Transparent depth texels are extended from the nearest foreground sample, preventing arbitrary background depth from pulling silhouette edges. Sources without depth can use normal integration; the visible main images all have authored depth.

Relief meshes use four-HD-pixel grid cells and full-resolution alpha coverage. Depth scale is provisionally 0.5 HD pixels per byte, consistent with the earlier reconstruction estimate, **not a recovered metric scale**. Surfaces are lifted along the canonical ray so depth does not shift their orthographic footprint. Perspective orientation retains the source's 32-degree elevation and selects native directional frames once per head, shared between eyes. Non-directional buildings remain single-view reliefs.

Lighting uses the stored sun direction `[-0.220336, 0.890320, 0.398475]`, decoded normals, specular response, bright, AO, emissive, and team paint. A prelit residual retains the canonical diffuse appearance while light/view changes affect the material response. The inherited Lambert fit has R² 0.0886; it is an approximate direction, not a newly validated physical lighting calibration. A bounded relighting floor avoids negative residuals turning an entire surface black.

`visible-type-audit.json` and `type-paint-audit.png` cover all **28 visible types**. Every main image has normals and depth, and every corrected source has nonblack foreground color. The scene has 31 non-shadow image layers, including an overlay without normals. This source-pixel audit does not assert that every dark on-screen pixel is erroneous or fully calibrated.

## Canonical comparison

Frozen native input: `m7/desktop-final/frame-00019.json`. Final render: `canonical-depth/frame-00003.png`, 1440×900 at 32 degrees, view height 12, camera 32,24. Reference: independent HD compositor, not an actual Remastered client framebuffer. No registration search, crop optimization or exposure correction is applied.

| Region | SSIM | >= 0.85 |
| --- | ---: | --- |
| Top left | 0.857816 | yes |
| Top middle | 0.886722 | yes |
| Top right | 0.869301 | yes |
| Bottom left | 0.867028 | yes |
| Bottom middle | 0.890707 | yes |
| Bottom right | 0.898492 | yes |

Maximum projected crop-origin error: **0.500015 px**, across 30 fully visible main sprites. `canonical-comparison.md`, the scores JSON and `acceptance.json` contain the table and detailed alignment rows. This is the existing crop-origin protocol; it is not a pixelwise silhouette correspondence metric.

## XR evidence

`final-ne/`, `final-nw/`, `final-sw/`, `final-se/` contain real Metal swapchain captures. Each `frame-NNNNN.png` has a matching `frame-NNNNN-right.png` and both metadata files. `acceptance.json` identifies final pairs, hashes, camera poses, physical eye separation and shared frame selections. All four views were inspected in the running simulator without restarting it.

The isolated runtime copy requests **90 Hz** at 1512×1680 per eye (encoded at 75% resolution). The four visual runs used software encoding because the isolated copy initially lacked the installed helper; their displayed receive rates are not used as the final performance result. The helper was then restored in the private copy for the hardware-backed benchmark. The shared D2R runtime configuration was not changed.

Stereo pairs establish distinct eye rendering and shared directional selection. Short keyboard translation attempts did not change the recorded head pose; a continuous tracked head-translation sweep is **not verified**. A physical headset was not tested.

## Performance

See `eight-player-metrics.json` and `eight-player-final.log`. The fixture contains **1,728 objects**: eight players × (200 army units + 16 buildings), rendered in tabletop mode. All objects are loaded; normal view-frustum culling still applies. The fixture replicates recorded native image chains and is frozen: it does not measure live simulation, animation-frame churn or eight-player networking.

Timing covers consecutive left-eye frame starts, CPU encoding of both eyes, and summed Metal GPU execution for both eyes. Warm-up samples are excluded by the metrics collector. PNG recording is disabled. Simulator streaming and ordinary background applications remain running. Final numbers are in the private acceptance report.

The final hardware-backed run on an Apple M4 Pro (48 GiB, x86_64 viewer) **fails 90 Hz**. Every measured frame exceeds the 11.11 ms budget.

| Measurement | Mean ms | p95 ms | Samples |
| --- | ---: | ---: | ---: |
| Frame interval | 75.146 | 117.511 | 1080 |
| Stereo CPU encoding | 58.499 | 73.250 | 1080 |
| Stereo GPU execution | 27.283 | 31.063 | 1080 |

Mean frame rate equivalent: 13.31 fps. CPU encoding and GPU execution overlap and must not be added together. The simulator displayed approximately 14 received fps, with zero displayed loss/errors/drops during inspection.

## Verification and remaining gaps

- Asset-venv Python main suite: **245 passed, 60 subtests passed**.
- Asset-venv native Python suite: **32 passed**.
- Rust suite: **14 passed**.
- Desktop and x86_64 XR builds succeed; **89 runtime checks pass in each**.
- No `devplay.py` process was found at safety checks. D2R was not stopped. Only Brood War viewers were restarted. `asset-upgrades/` was not modified. No push was performed.

Open acceptance items:

1. Cliff faces still show stretch and low-angle distortion. Probed terrain normal/height filenames were absent; this is a bounded path probe, not proof that no other terrain representation exists in CASC. VF4 reconstruction and denser triangles do not recover the original cliff geometry or unseen surfaces.
2. Relief adds real depth and stereo disparity but is not a closed, fully volumetric model. Static building reliefs retain source-view bias, exposed edges and contact/occlusion artifacts. Shadows remain owned 2D ground layers with provisional opacity.
3. The sun fit and depth scale are approximate. Canonical similarity is protected through a prelit residual, not independent physical-material calibration.
4. Continuous head translation, live animation stress, multi-map acceptance and physical-headset comfort remain unverified.
5. The 90 Hz requirement fails the measured full-scene run. Instancing/batching, material/texture reuse, visibility work and an arm64 viewer path need further investigation.

## Reproduce

Use the native runtime's `asset-venv/bin/python`:

```sh
RUNTIME="$HOME/.local/share/soju/broodwar-native"
"$RUNTIME/asset-venv/bin/python" tools/broodwar_native/prepare_relief.py \
  "$RUNTIME/3d-proof/m7/desktop-final/frame-00019.json" --all
python3 tools/broodwar_native/build_scene.py
python3 tools/broodwar_native/build_scene.py --xr
"$RUNTIME/asset-venv/bin/python" tools/broodwar_native/report_m9.py
```

For new captures use a fresh directory. Four camera/yaw pairs: NE `30,26 / 2.35619449`, NW `26,26 / -2.35619449`, SW `26,30 / -0.7853981634`, SE `30,30 / 0.7853981634`. Use `--xr --snapshot ... --record-stereo --record-dir ...`. Reconnect the existing simulator after restarting only the viewer; do not take over ports belonging to another session.
