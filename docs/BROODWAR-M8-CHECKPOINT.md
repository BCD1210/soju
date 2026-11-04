# M8 rendering checkpoint

The requested desktop diagnostic gates pass for the recorded M7 frame-19 scene at the same 1440×900, 32-degree camera and zoom. Four final viewpoints were inspected through the running OXRSys Simulator. These results use the unchanged independent HD compositor, not an actual Remastered client framebuffer.

Private evidence root: `~/.local/share/soju/broodwar-native/3d-proof/m8/` (`runtime/3d-proof/m8/`). No game assets or captures are tracked.

## Commits

- `0ad5547`: read-only native snapshot replay, including fog and creep; gameplay commands are disabled during replay.
- `7ddaec8`: shared terrain relief, original HD cliff-tile UVs, native-to-world anchor projection and inverse command-hit mapping.
- `bd349c1`: correct creep edge UV orientation and choose from the actual 13 valid variants rather than applying an extra modulo 16.
- `9df59c6`: observer-dependent directional frames, ground contact, native HD shadow planes, stereo-shared head selection, camera-scale stability and projected sprite-bound measurements.

## Desktop measurements

Frozen input: `m7/desktop-final/frame-00019.json`. Final output: `desktop-final/frame-00003.png`, with matching state, reference PNG, reference provenance and scores JSON. The frozen input removes simulation timing as a comparison variable. It is visibly labeled as snapshot replay in capture metadata and cannot issue native commands.

| Region | SSIM | Pass >= 0.85 |
| --- | ---: | --- |
| Top left | 0.860812 | yes |
| Top middle | 0.888032 | yes |
| Top right | 0.872400 | yes |
| Bottom left | 0.870716 | yes |
| Bottom middle | 0.893741 | yes |
| Bottom right | 0.902646 | yes |

Held-out affine luminance fit: **R² 0.913527**, gain 0.907748, offset 0.013426. The fit is reported, never applied to the scene. Alternating fixed 16-pixel training/evaluation blocks and the original six regions are unchanged. No crop search, image registration, exposure adjustment or reference-image editing was performed.

All **30 fully visible main sprites** have maximum projected crop-origin error **0.500015 px** against the independently rounded compositor placement. Offscreen or clipped main images are excluded explicitly. See `spriteAlignment` in the final scores JSON; the measure includes the Zerg row.

## Terrain reconstruction

The old mesh added zero-UV-area vertical walls to megatiles that already contain cliff artwork. M8 uses shared heightfield corners and samples the original megatile over each surface triangle. There is no second extruded wall and no repeated vertical edge texel. Creep uses that same surface and the corrected image orientation.

The reconstruction uses a fixed canonical 32-degree projection: world Z = native map Z + height / tan(32°). This preserves native HD map footprints at the comparison camera while retaining VF4-derived relief in perspective. It is an explicit reconstruction convention, **not recovered Blizzard geometry**. Ground height interpolation follows the actual triangle diagonal. Commands invert the projection, and commander scale changes preserve the native ground location.

## XR verification

The final four runs use the same built XR binary and frozen native scene. The existing simulator was reconnected, never restarted. Each capture is the actual Metal swapchain left eye, not a desktop screenshot. Camera pose and every billboard frame/mirror choice are in the sidecar.

Target: Marine at native (896, 896), image 239, native idle frameset 68. Its animation group remains fixed as the observer changes position.

| View | Head azimuth | Frame | Mirrored | Observed receive fps | Capture |
| --- | ---: | ---: | --- | ---: | --- |
| NE | 135° | 80 | no | 46 | `xr-angles/ne.png` |
| NW | -135° | 80 | yes | 40 | `xr-angles/nw.png` |
| SW | -45° | 72 | yes | 52 | `xr-angles/sw.png` |
| SE | 45° | 72 | no | 44 | `xr-angles/se.png` |

All four observations reported zero loss, errors and drops. `xr-four-angles.json` binds the captures to their source frames and hashes. The target's projected crop baseline meets its ground anchor in all four views. Foreground units, their ground shadows and changing directional artwork were inspected in the simulator; the wrist panels remain in the captures.

Directional images use the native flag, frameset and mirrored half-turn. Fixed-frame buildings are not misinterpreted as directional animations. Selection and orientation update once from the head center before either eye renders. Upright perspective billboards pivot at the HD frame's lower crop edge; the original shadow image remains on the ground, independent of head motion and flight height. Its source projection is undone before placing the plane.

`shadow-coverage.json` measures the actual drawfunc-10 images in this snapshot: **29/29 frames contain fractional alpha coverage**. This retains the owned HD soft edge rather than substituting a hard ellipse. Opacity remains the existing 0.5 multiplier.

## Validation

- Asset-venv main Python suite: **245 passed, 60 subtests passed**, `tests-main.log`.
- Asset-venv native Python suite: **28 passed**, `tests-native.log`.
- Rust suite: **14 passed**, `tests-rust.log`.
- Desktop and x86_64 XR builds succeeded, `build-final-desktop.log`, `build-final-xr.log`.
- Desktop and XR runtime checks: **78 passed each**, `runtime-final-*.json`.
- Private-copy mutations failed at the intended assertions: ignoring observer azimuth fails directional selection; reverting shared corners fails terrain continuity. `mutation-direction.log`, `mutation-relief.log`. Production files and binaries were not mutated.
- `acceptance.json` records source snapshot and binary hashes, regional scores, sprite alignment and XR evidence.
- No `devplay.py` process was found at the safety checks. D2R was not stopped. Only the Brood War viewers were restarted; the native game bottle did not need a restart. `asset-upgrades/` was not changed. No push was performed.

## Remaining limits

- This passes the specified frame/zoom diagnostic thresholds, not full client-renderer equivalence. The reference's existing fog, creep seed, draw-function and sorting limitations remain.
- The terrain is reconstructed relief. Steep surfaces and views away from the canonical camera still show oblique texture distortion; physical cliff geometry is not recovered from the 2D artwork. Multi-map visual acceptance remains open.
- HD directional sprites are still billboards. Non-directional buildings retain their owned static views; fully volumetric replacements remain behind the existing fidelity gate.
- The luminance R² is not a physical sun-direction or shadow-opacity fit. The shadow opacity multiplier remains provisional.
- XR verification used a frozen native scene, not newly exercised live gameplay. Simulator performance (40–52 received fps in the final observations) and image quality do not establish physical-headset comfort or performance.

## Reproduce

```sh
RUNTIME="$HOME/.local/share/soju/broodwar-native"
python3 scripts/broodwar-native.py scene-build
python3 scripts/broodwar-native.py scene-build --xr
"$RUNTIME/soju-bw-scene" \
  --snapshot "$RUNTIME/3d-proof/m7/desktop-final/frame-00019.json" \
  --top-down --camera 32,24 --view-height 12 --elevation 32 \
  --record-dir "$RUNTIME/3d-proof/m8/new-desktop" --record-seconds 4
"$RUNTIME/asset-venv/bin/python" tools/broodwar_native/reference_scores.py \
  "$RUNTIME/3d-proof/m8/new-desktop/frame-00003.json"
```

Use fresh capture directories. For XR use the existing runtime manifest with `soju-bw-xr --xr --snapshot ...`; the final camera/yaw pairs are NE `30,26 / 2.35619449`, NW `26,26 / -2.35619449`, SW `26,30 / -0.7853981634`, SE `30,30 / 0.7853981634`. Reconnect the running simulator after restarting only the viewer. Never take over a runtime port owned by another session.

Directional conventions: [OpenBW image flags](https://github.com/OpenBW/openbw/blob/master/game_types.h#L383-L392) and [frame selection](https://github.com/OpenBW/openbw/blob/master/bwgame.h#L12674-L12709).
