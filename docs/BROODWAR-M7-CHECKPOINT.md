# M7 checkpoint

M7 is **not accepted** against the full requested criteria. The deterministic HD
compositor, measurements, model fallback gate and targeted fixes are implemented.
Exact client compositing, a lighting fit above R² 0.8, complete native command-card
parity and Remastered-equal cliff appearance remain unfinished. No client HD frame
is required or awaited by this implementation.

Private evidence root: `~/.local/share/soju/broodwar-native/3d-proof/m7/`
(`runtime/3d-proof/m7/` in the commands below).

## Changes

- `7e36b1b`: export team masks and per-image GRP anchor dimensions; deterministic
  HD terrain/sprite compositor with native frame selection, mirroring, reversed
  image-chain order, source hashes and explicit unsupported-state reporting.
- `4861a0f`: clip crossing ramp edges at the height intersection. Both exposed
  triangles are closed without overlapping walls; use a non-degenerate triangle
  for wall normals. Plateau/ramp tests cover both crossing orientations.
- `7724479`: make every exposed commander button reachable by a unique key on its
  page; remove movement from larvae/eggs and commands from neutral objects.
  Audit all 228 DAT types in 456 base/alternate states. Numeric keys resolve
  missing/conflicting shortcuts; they are not claimed to be native shortcuts.
- `8da94a2`: `assets-review --installed` renders installed models with fixed
  orthographic placement, measures every available directional candidate, writes
  the Dots queue, and binds live approval to the GLB hash and catalog revision.
  Failed/unmeasured models use owned HD image chains. Core Image applies team
  masks; wrist panels render above world sprites. Capture sidecars retain native
  state, camera, fog and creep data.
- `e2ad264`: camera-adapted reference projection, captured fog/creep approximation,
  six fixed-region scores and a held-out affine brightness fit. No score-driven
  image registration, crop search or per-direction model resizing.

## Live evidence

| Evidence | Path relative to evidence root |
| --- | --- |
| Final desktop, 32-degree orthographic camera | `desktop-verified/frame-00009.png` |
| Matching native state and camera | `desktop-verified/frame-00009.json` |
| Camera-adapted 2D reference | `desktop-verified/frame-00009-reference.png` |
| Reference provenance and compositor limitations | `desktop-verified/frame-00009-reference.json` |
| Regional comparison and fit | `desktop-verified/frame-00009-scores.json` |
| Live OpenXR Metal eye captures, with SCV wrist card | `xr-verified/frame-*.png` and JSON sidecars |
| Earlier simulator run with readable panels | `xr-final/frame-*.png` |
| All installed model renders | `model-renders/` |
| All model scores and failures | `model-fidelity.json` |
| All command cards/states | `command-audit.json` |
| 29 native shadow-frame coverage measurements | `shadow-softness.json` |

Desktop and simulator were inspected live. The simulator received the updated
scene and displayed the SCV card, resources and HD units. A sampled final-build
run showed 1,178 received frames, zero reported loss/errors/drops and about 18 fps.
That is not headset-performance acceptance. Only the dedicated Brood War viewer
processes and bottle were started/restarted; the simulator was connected without
restarting an existing simulator process. No `devplay.py` process was found, and
D2R was not stopped.

## Scores

The reference is an independent compositor of owned HD assets, **not yet an exact
Remastered client renderer**. Its 32-degree output projects terrain/positions to
the recorded viewer camera. Full-frame regions are fixed thirds by halves,
left-to-right, top-to-bottom. Brightness is mean encoded Rec.709 luminance;
contrast is its standard deviation, both on a 0–1 scale.

| Region | SSIM | Reference brightness | Viewer brightness | Reference contrast | Viewer contrast |
| --- | ---: | ---: | ---: | ---: | ---: |
| Top left | 0.8520 | 0.1079 | 0.1068 | 0.0978 | 0.1035 |
| Top middle | 0.8819 | 0.1150 | 0.1123 | 0.0795 | 0.0853 |
| Top right | 0.7404 | 0.1210 | 0.1189 | 0.0876 | 0.0947 |
| Bottom left | 0.4403 | 0.1437 | 0.1432 | 0.0750 | 0.0796 |
| Bottom middle | 0.4620 | 0.1612 | 0.1675 | 0.0765 | 0.0743 |
| Bottom right | 0.6405 | 0.1641 | 0.1647 | 0.1119 | 0.1161 |

Affine brightness fit: gain **0.733111**, offset **0.036075**, held-out
**R² 0.593411**. Fit/evaluation use alternating fixed 16-pixel blocks. The required
R² > 0.8 is **not met**. The fit is not applied: compositing and geometry residuals
must not be hidden by changing exposure. HD billboards use prelit color, so these
pixels do not independently identify a 3D sun direction. No sun-direction or
projected shadow-softness fit is claimed. The source shadow coverage edges were
measured, but their opacity remains a provisional 0.5 multiplier.

Installed-model review: **0 passed, 110 measured failures, 118 unmeasured or
unavailable entries** across all 228 DAT types. See
[BROODWAR-MODEL-QUEUE.md](BROODWAR-MODEL-QUEUE.md). Scores include installed scale,
pivot and materials, not just mesh shape. First directional groups are candidate
idle poses; they have not all been mapped to native actions. Non-directional
buildings have one static comparison. Missing scores are never treated as zero
or a pass. Normal-game frequency ordering is an explicit priority estimate, not
match telemetry. Nothing in `asset-upgrades/` was changed.

## Validation

- Asset-venv main Python suite: **245 passed**, `tests-main-final.log`.
- Asset-venv native Python suite: **28 passed**, `tests-native-python.log`.
- Desktop and XR runtime checks: **58 each**, `runtime-desktop.json`, `runtime-xr.json`.
- Native Rust tests: **14 passed**, `rust-tests.log`.
- Live DAT card audit: **228 types / 456 states**, no empty or colliding page keys.
- Both desktop ARM64 and XR x86_64 builds succeeded.
- Team-mask and image-chain-order tests were checked against deliberately broken
  implementations; both mutations failed, and original sources were restored.

## Remaining acceptance gaps

1. Native unit sprite elevation and exact equal-depth ordering are not exported.
   Drawfunc 9 and other special blend/remap functions, client fog smoothing,
   creep variant seed, terrain elevation projection and ground-shadow perspective
   are not fully reproduced. The compositor reports `clientEquivalent: false`.
2. Brightness means now closely track the HD reference, but full-scene structure
   and the lighting-fit threshold fail. Sun and 3D shadow calibration remain open.
3. The crossing-edge hole is fixed; cliff texture stretching and all-map seam
   appearance are not accepted. The extruded walls remain visibly different from
   the 2D terrain artwork.
4. The card audit proves reachability of implemented commands, not completeness.
   Original buttonsets/prerequisite predicates, hero-specific abilities,
   carrier/reaver ammunition, archon merges, and upgrade-level pricing remain.
5. HD billboards prevent below-threshold models from appearing, but their remaining
   compositor differences mean the overall game is not yet Remastered-equal.
   Missing source images, independent sprites/team tint, and transient effects
   still need coverage beyond the current fixture.
6. Simulator verification is not physical-headset verification; observed stream
   performance is below a comfortable headset target.

## Reproduce

```sh
runtime/asset-venv/bin/python tools/broodwar_native/prepare_world_sprites.py
python3 scripts/broodwar-native.py scene-build
python3 scripts/broodwar-native.py scene-build --xr
python3 scripts/broodwar-native.py assets-review --installed
runtime/soju-bw-scene --command-audit runtime/3d-proof/m7/command-audit.json
runtime/soju-bw-scene --top-down --camera 32,24 --view-height 12 --elevation 32 \
  --record-dir runtime/3d-proof/m7/new-desktop --record-seconds 10
runtime/asset-venv/bin/python tools/broodwar_native/reference_scores.py \
  runtime/3d-proof/m7/new-desktop/frame-00009.json
```

Use a fresh capture directory; capture files are never overwritten. Reload the
viewer after an installed-model review to load the new approval report. Review
uses private assets and writes only code-generated scores/queue text to the
repository. No game assets were committed and nothing was pushed.

Format references: [Animosity layer and GRP anchor documentation](https://github.com/neivv/animosity#anim-layers)
and [OpenBW image-chain draw order](https://github.com/OpenBW/openbw/blob/master/ui/ui.h).
These explain the implemented conventions; neither establishes exact SC:R shader
parity for this compositor.
