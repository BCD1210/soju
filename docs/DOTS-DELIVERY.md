# Dots delivery contract

Work in `~/.local/share/soju/broodwar-native/asset-upgrades/<unit>/`.
Keep game art, source files, textures and renders outside git. The intake command
reads this folder and stages a separate copy; it never edits the delivery.
The current repository is `~/Projects/soju-broodwar`.

Deliver each model as a standard embedded GLB 2.0 plus a same-basename `.blend`
with packed textures. Blender sources are archived, never executed by intake.
Use right-handed glTF coordinates: +Y up, +Z forward, ground/foot origin within
2% of height. Apply object transforms. One terrain tile is one runtime scene
unit. Marine target height is 1.748; SCV is 1.878. Specify `targetHeight` explicitly.

LOD0: 15,000–40,000 triangles. LOD1: at most 15,000 and fewer than LOD0.
Textures must be embedded PNG/JPEG, no larger than 4096 on either axis.
Use unique bone names and independently instantiable skins. Clips must be named
and mapped explicitly; Marine requires idle/walk/attack, SCV additionally
work/carryIdle/carryWalk. Supply death when authored. Do not rename bones without
a declared mapping. `native` means a self-contained clip-driven rig with no
retargeting dependency. A custom mapping must identify original and delivered
bone names; it is not an instruction to retarget automatically.

Use diffuse alpha as team mask (`alpha`), or a separate grayscale mask texture
(`texture`, `teamMaskTexture`). White means team color, black means unchanged.
Alpha masks are color data, not cutout opacity. Check all eight player colors
in the native renderer. Supply `provenance.json` describing sources and rights.

Example `catalog.json` (add SCV as key `7`):

```json
{
  "schemaVersion": 1,
  "models": {
    "0": {
      "file": "marine.glb",
      "targetHeight": 1.748,
      "footPivot": true,
      "yawDegrees": 180,
      "faceHeading": true,
      "teamMask": "alpha",
      "animations": {"idle": ["Idle"], "walk": ["Walk"], "attack": ["Fire"]},
      "requiredAnimations": ["idle", "walk", "attack"]
    }
  }
}
```

Example `delivery.json`:

```json
{
  "models": {
    "0": {
      "up": "+Y",
      "forward": "+Z",
      "skeletonMapping": "native",
      "lod1": "marine-lod1.glb"
    }
  }
}
```

Run from the repository:

```sh
python3 scripts/broodwar-native.py assets-review /absolute/path/to/delivery
python3 scripts/broodwar-native.py assets-rollback
```

The command uses the private asset Python environment and Blender. It returns
nonzero on rejection. Output lives in `runtime/3d-proof/m3/intake/<timestamp>/`:
`review.json`, the staged delivery, per-unit `render-job.json`, geometry audit,
17 `renders/` and side-by-side `comparisons/`, 36 perspective `turntable/` frames,
`closeup.gif`, and actual SceneKit checks/views under `native/`.
The source archive includes hashes. A pass activates the existing atomic release
installer, which runs native rig checks again and retains the previous catalog
for rollback. A failed review does not change the active release.

The fixed camera comes from the measured Marine `rigged-e32` and SCV
`rigged-dense` experiments. Exact dimensions, pixels per unit, target, elevation
and all 17 azimuths are copied into each report. Marine elevation is about 32°;
this is a fit to different source geometry, not an independently established
Remastered camera. Intake applies one height normalization and foot translation
into that calibration space. It never aligns/resizes individual views.
Diffuse comparisons use an unlit material, premultiplied black RGB, and 7×7 SSIM
averaged over the union of foreground pixels. Every direction must reach IoU
0.95 and SSIM 0.85. Existing baseline models are expected to fail.

Perspective turntables use a 50mm camera and neutral area light. These are
close-up desktop proxies for VR viewing, not a claim of headset validation.
Forward-axis meaning, anatomy, deformation quality and artistic acceptance need
inspection of the native views. Numeric similarity alone cannot certify them.
Only Marine/SCV currently have calibrated 17-view reference sets. Other types
are rejected until measured references are available. LOD1 is audited and
archived; runtime distance switching remains a separate renderer requirement.
