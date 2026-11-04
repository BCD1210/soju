# Rigged Remastered fitting experiment

Neither pilot passes. No fitted model is installed or approved; SCEvo remains active. The best measured candidates are visibly distorted and unsuitable for release. The measurements below describe the implemented optimizer, not a proof that the requested fidelity is impossible.

## Measured experiments

Every row is the minimum across 17 independent Blender renders at the original canvas resolution. The gate remains IoU >= 0.95 and foreground diffuse SSIM >= 0.85 **in every direction**. All rows pass 0/17 directions.

| Type | Experiment | Min IoU | Min diffuse SSIM |
| --- | --- | ---: | ---: |
| 0 | `rigged/baseline` | 0.4818 | 0.1854 |
| 0 | `rigged/fit` | 0.7254 | 0.2550 |
| 0 | `rigged-dense/fit` | 0.7300 | 0.3076 |
| 0 | `rigged-e32/arap` | 0.6791 | 0.2567 |
| 0 | `rigged-e32/fit` | 0.7900 | 0.3254 |
| 7 | `rigged/baseline` | 0.6204 | 0.2119 |
| 7 | `rigged/fit` | 0.7695 | 0.2662 |
| 7 | `rigged-dense/arap` | 0.6866 | 0.2292 |
| 7 | `rigged-dense/fit` | 0.6964 | 0.2467 |
| 7 | `rigged-dense/seeded` | 0.7937 | 0.2777 |
| 7 | `rigged-e32/fit` | 0.6330 | 0.1648 |

Selected diagnostic candidates: Marine `0/rigged-e32/fit`; SCV `7/rigged-dense/seeded`, under `runtime/models-remastered/`. These have the strongest combined measured results of these trials, not accepted art. Each contains `model.glb`, `candidate.blend`, seven or six 2048-square projected layers, `validation.json`, `renders/{00..16}.png`, and `comparisons/{00..16}.png` (Remastered | ours | absolute difference). Earlier hull outputs remain intact.

## Conditional camera estimates

The source-mesh search grids both yaw signs, eight absolute yaw offsets, and elevations 15–65 degrees, then refines shared scale, target, elevation and yaw with Powell optimization. Individual yaw refinements are bounded to +/-5.625 degrees. There is no per-view translation, scale, image registration or resizing. Orthographic projection is a hypothesis; no perspective focal length is recovered.

Free-elevation fits prefer 22.702 degrees for Marine and 51.664 degrees for SCV. A separate approximately 32-degree hypothesis improves the fitted Marine but worsens SCV. This disagreement and source-pose sensitivity mean the physical sprite camera is **not uniquely solved**. The selected Marine uses the 32-degree hypothesis, not a claim of recovered physical elevation.

| Type | Elevation | Pixels / source unit | Orthographic span (wider dimension) | Target XYZ |
| --- | ---: | ---: | ---: | --- |
| 0 | 32.000311 | 126.274659 | 2.478724 | 0.119996, 0.130573, 0.233246 |
| 7 | 51.664466 | 129.852979 | 2.217893 | -0.065857, 0.050135, 0.504885 |

Coordinates are Blender Z-up, unchanged source rig units. Complete camera searches and results are in each experiment parent’s `camera.json`. Selected per-direction yaws (degrees):

| Direction | Marine | SCV |
| ---: | ---: | ---: |
| 0 | 247.849710 | 4.582870 |
| 1 | 236.224710 | -5.542130 |
| 2 | 226.474710 | -19.417130 |
| 3 | 217.099710 | -28.042130 |
| 4 | 213.724710 | -37.042130 |
| 5 | 191.224710 | -45.292130 |
| 6 | 179.974710 | -65.167130 |
| 7 | 169.849710 | -70.417130 |
| 8 | 162.349710 | -78.292130 |
| 9 | 157.474710 | -89.167130 |
| 10 | 146.224710 | -100.417130 |
| 11 | 134.599710 | -118.042130 |
| 12 | 123.724710 | -133.042130 |
| 13 | 112.099710 | -142.042130 |
| 14 | 99.724710 | -155.167130 |
| 15 | 87.724710 | -167.917130 |
| 16 | 78.349710 | -178.417130 |

## What limits the current result

- **Geometry/pose mismatch:** source idle Marine carries a different rifle/arm pose; `Stand Ready_full` was selected for the final trials. Normal-displacement contour fitting with Laplacian regularization improves silhouette but creates spikes and stretched faces. Dense fitting uses 27,360 Marine triangles and 43,584 SCV triangles, retaining 43/41 bones and interpolated skin weights. Neither new structural parts nor Remastered action poses are recovered.
- **Measured shape tradeoff:** selected Marine/SCV maximum edge stretch is 20.28x/28.19x, with 2,115/2,849 triangle normals reversed relative to the starting pose. These counts diagnose distortion; they are not exact self-intersection counts. ARAP reduces maximum stretch to 4.10x/3.49x and normal reversals to 405/1,048, but minimum IoU falls to 0.6791/0.6866. A better silhouette alone does not establish an acceptable animated surface.
- **Observed optimization plateau:** selected last-30-iteration mean IoU gain is 0.000585 for Marine (300 iterations), and 0.000236 for SCV (200 seeded iterations). This is a local optimizer limitation, not a global optimum or impossibility proof.
- **Rendering agreement:** software fitting silhouettes vs independent Blender alpha have minimum IoU 0.99236/0.99333. Camera-coordinate/raster disagreement is too small to explain the final fidelity failure.
- **Texture correspondence and source shading:** shared 2K UV atlases average incompatible observations. Weighted diffuse disagreement is 26.39/33.62 RGB byte values RMS at multiply observed texels. Only 42.95%/45.20% of occupied UV texels have accepted observations; remaining texels use explicitly reported nearest fill. Source diffuse is prelit, so this residual mixes geometric correspondence errors with view-dependent shading. It is not evidence that atlas resolution alone is the problem.
- **Depth:** all 17 views for both selected models fail the <=5-pixel affine-depth residual check. Marine residuals are 8.91–11.87 pixels; SCV 10.18–25.18. Depth was audited but not added as a fitting term. A low residual alone would still need cross-view consistency before use.
- **Available views:** only 17 owned observations are used. Marine foreground bounds range 56–98 pixels wide and 90–103 high; SCV 108–133 wide and 94–169 high. No top/underside or independent mirrored-side measurements exist in these groups. These constrain recoverable detail but do not establish a numerical upper bound on the requested gate.
- **Topology:** inherited segmented source meshes are not watertight after positional welding; SCV’s dense trial also exceeds the earlier 40k triangle target. No final LOD was produced for these rejected candidates.

## Animation and desktop evidence

All 24 Marine and 14 SCV base clips survive export. Their original GLBs do not contain death. Separate locally preserved SCEvo death variants supply a first-pose-aligned named-bone transfer: 23/43 Marine bones and 11/41 SCV bones. Death frame-zero alignment residual is below 0.000001 source units. Unmatched appendages follow their parents; detached debris and particle effects are not reproduced.

`runtime/3d-proof/rm/rigged-animation/asset-checks.json` records actual SceneKit playback of the fitted GLBs. Attack, walk and non-looping death move for both; SCV work moves. Second-instance transforms remain unchanged. Captures are `{0,7}-{attack,death,walk,idle}-{0..3}.png`, plus `7-work-{0..3}.png`. Contact sheets include `0-death-strip.png`, `7-death-strip.png`, and attack/work strips. Bone motion passes; visual deformation fails. This is renderer playback verification, not a rerun of native combat/death events.

`runtime/3d-proof/rm/rigged-live/` contains `live-{0,7,106}.png` and `live-proof.json`, captured from the advancing original client at frame 66026. Type 106 remains fallback. The isolated `models-remastered/rigged-preview/catalog.json` retains required animation checks and is explicitly marked failed acceptance. Active releases were not changed and no game orders were issued.

OpenXR port 9946 remained held by D2RLoader/oxrsys-en at the final check. Those processes were untouched. No new OpenXR or physical-headset capture was made, and there are no passing models to display.

## Reproduce

Use the private asset environment with `requirements-remastered.txt` (including the tested Numba rasterizer dependency):

```sh
~/.local/share/soju/broodwar-native/asset-venv/bin/python \
  tools/broodwar_native/remastered_rig_pipeline.py
```

The full command regenerates camera, coarse, dense, fixed-elevation, seeded and ARAP trials. It exits 1 on the failed pilot gate and never promotes assets, starts games, stops sessions, or touches `asset-upgrades/`. `--report-only` revalidates the existing artifacts; it was run and returned 1 as expected. Each numerical and Blender stage was exercised in this run.

```sh
~/.local/share/soju/broodwar-native/asset-venv/bin/python \
  tools/broodwar_native/remastered_rig_report.py --preview --coverage
~/.local/share/soju/broodwar-native/soju-bw-scene \
  --model-dir ~/.local/share/soju/broodwar-native/models-remastered/rigged-preview \
  --model-proof --animation-proof --model-types 0,7 \
  --proof-dir ~/.local/share/soju/broodwar-native/3d-proof/rm/rigged-animation
```

Validation: 27 existing Python/Swift regression tests, 10 reconstruction/numerical tests, desktop build, XR build, and all 24 native runtime checks pass. The isolated asset playback check passes both pilots. Passing engineering checks do not waive failed art gates.

## Remaining work

Physical camera calibration, shape/pose correspondence, stable deformation, texture fidelity, depth fusion, complete death effects, action fidelity to Remastered, LODs, live combat event revalidation, OpenXR captures and physical-headset acceptance remain unfinished. Pilot failure prevents bulk processing: 0/228 types accepted; the remaining 226 types were not fitted in this run. The source inventory remains 155 distinct main images, including 111 non-directional/insufficient-view entries. See [coverage](BROODWAR-REMASTERED-COVERAGE.md).

The next useful experiment needs correspondences between named anatomical/rig parts and the source layers, and an independently constrained camera/pose, before allowing large surface deformation. Further unconstrained subdivision or atlas enlargement does not address the measured failure.
