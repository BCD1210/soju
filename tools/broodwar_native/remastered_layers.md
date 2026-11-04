# Owned Remastered layer evidence

Audit command (private runtime Python needs Pillow and NumPy):

```
~/.local/share/soju/broodwar-native/asset-venv/bin/python tools/broodwar_native/remastered_layers.py
```

The active `scene-assets/manifest.json` has zero images (terrain-only). The preserved
source is `terrain-releases/legacy-1793765866898045000/manifest.json`. The audit
records its SHA-256, every layer hash, foreground-only RGBA ranges/means/deviations,
and first-frame/channel crops in `runtime/3d-proof/rm/`. Assets stay outside git.

## Channel interpretation

The [Animosity author’s documentation](https://github.com/neivv/animosity#readme)
describes diffuse as prelit colour, bright as the real-time-lighting base,
normal as signed X/Y in R/A with positive Z reconstructed, AO in G, and depth in
A increasing towards the camera. Emissive RGB adds light; alpha is not emission.
These semantics are consistent with the actual decoded DDS samples below; they
do not establish a calibrated camera or metric depth scale.

| Image | Frames | Layers | Full first-crop depth range |
| --- | ---: | --- | --- |
| Marine 239 | 229 | diffuse, bright, teamcolor, emissive, normal, specular, ao_depth | 24–76 |
| SCV 247 | 51 | diffuse, bright, teamcolor, normal, specular, ao_depth | 0–98 |
| Command Center 275 (source alias) | 6 | all seven | 32–209 |
| Supply Depot 278 | 2 | all seven | recorded in layers.json |
| Geyser 344 | 8 | all except teamcolor and emissive | recorded in layers.json |
| Minerals 347,349,351 | 4 each | all except teamcolor | recorded in layers.json |

Normal G/B are zero, rather than a conventional RGB tangent-space normal. Normal
R/A vary across the body; decoding alpha as opacity would destroy valid normals.
AO R/B are zero; A varies even outside the foreground. Use **diffuse alpha** as
the surface mask, never AO alpha. Marine first-frame emissive RGB is all zero.
Specular alpha is 255 in the inspected first crops, not a roughness channel.
A specular-to-roughness conversion is therefore an approximation and must be
labelled as such. Team colour is an independent mask, not diffuse alpha.

## Layout and camera calibration limits

ANIM frame records supply atlas x/y, canvas ox/oy and crop w/h. Reconstruct the
original canvas with these offsets before comparing directions. Marine canvas
is 313×226 and SCV 288×282. Marine's first 17 crops vary from 58–102 pixels wide;
resizing each crop independently would erase the measured silhouette.

The first 17-frame groups of Marine and SCV are directional candidate poses.
Frame count alone does not prove animation labels or timing. In particular SCV
has only three groups of 17, not a complete Walk/Attack/Work/Death sequence.
Buildings/resources here have fewer than 17 frames: they are **not** multi-view
captures. Their animation frames must never be mislabelled as azimuths. Deaths,
worker tools, turrets and effects may be separate ANIM entries or image chains.
An iscript/image-chain mapping is required before claiming action coverage.

A preliminary camera estimate uses near-zero-X, upward normals on broad building
surfaces. The modal normal Y suggests elevation **33.12° for Command Center** and
**31.52° for Depot** (normal-space interpretation assumes image-up Y). Use ~32°
as a candidate, not a known camera constant. Unit-only modes disagree (Marine
26.81°, SCV 48.51°) because sloped armour is not a ground plane. No flat footprint
has yet independently verified the estimate. Small depth-gradient/normal fits
have correlations only 0.56–0.63 and scales 0.33–0.73 pixel per depth byte,
consistent with quantization/art detail; a unit metric scale is not established.

Normal-space evidence supports camera-space XY: R changes across screen-facing
curvature and the depth-gradient signs agree with -Nx/Nz and Ny/Nz. World-space
versus camera-space still needs a registered cross-view residual test. Recovering
a surface from these layers is an inverse-rendering experiment, not access to
the original meshes. Hidden undersides and action topology remain unobserved.
