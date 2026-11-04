# Remastered reconstruction experiment

The current rigged-mesh follow-up is documented in
[BROODWAR-RIGGED-FITTING.md](BROODWAR-RIGGED-FITTING.md). Its selected Marine/SCV
minimum IoU/SSIM are 0.7900/0.3254 and 0.7937/0.2777; neither passes.
The hull experiment below is preserved as historical evidence.

The requested Remastered-quality replacement is **not achieved**. These tools
produce and measure candidates; passing compilation or producing a GLB does not
approve their art. SCEvo remains the temporary fallback. No game assets are in git.

## Reproduce

Run from the repository using the existing private asset Python and Blender:

```
~/.local/share/soju/broodwar-native/asset-venv/bin/python \
  tools/broodwar_native/remastered_pipeline.py --resolution 224 --calibrate
```

Dependencies are recorded in `tools/broodwar_native/requirements-remastered.txt`.
The pipeline returns **1 on a failed pilot gate**. Blender Python failures also
return nonzero. Runtime outputs stay under `models-remastered/` and `3d-proof/rm/`.
It does not install assets, restart games, download artwork or write `asset-upgrades/`.

## Implemented stages

1. Inspect all seven possible layers from the preserved local ANIM extraction;
   preserve channel crops, ranges and source hashes. See
   [layer evidence](../tools/broodwar_native/remastered_layers.md).
2. Sweep camera elevation and azimuth sign using coarse silhouette intersections.
   The negative azimuth convention scores better. Mean-score peaks are 30° for
   Marine and 40° for SCV; a shared 32° candidate uses the building-normal evidence.
   This is not a solved calibrated camera. Coarse scores are not acceptance scores.
3. Intersect orthographic silhouette constraints at 224³ voxels using 17 measured
   directions and the game's 15 horizontal mirrors. Decode mirrored normal X with
   the opposite sign. Attempt per-direction depth affine fits; reject a view at
   >5 pixels RMS or a nonpositive depth scale. **All pilot depth fits were rejected**;
   the produced candidates are visual hulls, not successful depth reconstructions.
   Truncated depth fusion is implemented but has not produced a validated result.
4. Smooth the voxel boundary, remove sub-64-voxel fragments, extract a mesh,
   decimate toward 30k triangles, create an approximately 4k LOD and a UV atlas.
   Check manifold edges explicitly rather than inferring watertightness from format.
5. Bake 2K diffuse, bright, team mask, tangent normal, specular and available
   emissive textures using view-facing weights and voxel front-depth visibility.
   Specular-to-roughness is approximate. Unobserved texels are nearest-filled and
   their fraction is reported, not presented as recovered detail.
6. Re-render 17 source cameras in private Blender and save every comparison.
   Diffuse-only emission compares the prelit source diffuse without double lighting.
   This is a diffuse diagnostic, **not** an established PBR lighting match.
   SSIM is averaged over the union foreground, so empty background cannot inflate it.
7. Generate a 228-type owned-CASC inventory and a catalog that excludes failed
   candidates. Main-image direction flags are read from classic `images.dat`;
   [PyMS's primary format definition](https://github.com/poiuyqwert/PyMS/blob/master/PyMS/FileFormats/DAT/ImagesDAT.py)
   specifies the 999-entry graphics pointer and turnability arrays.

## Unfinished work and next experiments

- Recover a joint camera/depth scale and offset that explains corresponding views,
  using visible normal/depth consistency rather than a hull-front linear fit.
  Check original canvas pivots and per-direction pose consistency independently.
- Refine geometry against silhouette and normal residuals; the current hull does
  not meet the 0.95 per-view silhouette threshold. Higher triangle counts alone
  cannot fix inconsistent geometry or missing concavities.
- Improve visibility/UV transfer and test the actual lit material against the bright
  and normal layers. The current atlas does not reach 0.85 diffuse SSIM.
- Decode iscript and image-chain composition before labelling animation frames.
  No Walk/Attack/Work/Death retargeting or morph animation is implemented. No measured
  choice between those animation strategies has been made, and attack/death do not
  play on these candidates. Existing fallback skeletons were not mislabelled as
  reconstructed animation.
- Resolve non-directional buildings/minerals/geysers separately: repeated animation
  frames are not additional camera views. Their unseen sides are absent from these
  entries. The all-type pass is an **inventory**, not all-type reconstruction.
- Run the OpenXR preview once its shared streaming port is available. At inspection,
  port 9946 belonged to D2RLoader/oxrsys-en; those processes were left untouched.

## Isolated live inspection

```
python3 tools/broodwar_native/remastered_catalog.py --preview
~/.local/share/soju/broodwar-native/soju-bw-scene \
  --model-dir ~/.local/share/soju/broodwar-native/models-remastered/preview \
  --live-proof --proof-dir ~/.local/share/soju/broodwar-native/3d-proof/rm/live-desktop
```

The preview copies the fallback catalog and overrides only the pilot candidates.
It deliberately clears required animation checks **in this isolated preview only**;
its catalog says failed acceptance. The approved catalog remains empty. Normal
viewer launches continue using the existing active release.

Live proofs read the original client's private fixture state, use the real desktop
renderer and save frame/positions with PNGs. They issue no game orders. Terrain
queries are tiled to the native 64×64 limit. This is desktop live-state evidence;
it is not an OpenXR or physical-headset capture.

See [per-type coverage](BROODWAR-REMASTERED-COVERAGE.md) and each private
`models-remastered/<type>/validation.json` for final numeric results.

## Recorded outcome

Final mirrored-view candidates (minimum over the 17 Blender comparisons):

| Type | Triangles / LOD1 | Min IoU | Min diffuse SSIM | Passing views | Watertight |
| --- | ---: | ---: | ---: | ---: | --- |
| Marine 0 | 30,000 / 3,999 | 0.6803 | 0.1713 | 0/17 | yes |
| SCV 7 | 29,998 / 3,994 | 0.5407 | 0.1258 | 0/17 | no: 33 non-manifold edges |

Both have 2K atlases and zero animation clips. All candidates fail. The earlier
17-view-only experiment scored better (Marine minimum 0.7693/0.3224, SCV
0.7433/0.2826), but also failed every direction gate and had substantial unseen
surface defects. Adding mirrored constraints reduced original-view agreement;
mirroring is not additional independent geometric evidence. Neither experiment
establishes a coherent reconstruction of the source art.

Live desktop renderer evidence at original engine frame 34698:
`runtime/3d-proof/rm/live-desktop/live-{0,7,106}.png` and `live-proof.json`.
The report includes candidate hashes and the isolated preview catalog. Type 106
is still the fallback. The preview process was closed after capture; the game and
active model release were left unchanged. No OpenXR candidate frame was captured.

Validation: 27 existing Python/Swift tests, six reconstruction/gate tests,
24 native runtime checks, desktop build and XR build passed. The end-to-end
reconstruction command returned 1 as intended on the failed art gate. SCV's
remaining topology issue is reported rather than passed by the geometry check.

Coverage: 228 rows, 155 distinct main images, zero accepted types, two failed
pilots, 111 non-directional/insufficient-view entries and 115 not reconstructed.
Bulk geometry/material/animation reconstruction remains unfinished.
