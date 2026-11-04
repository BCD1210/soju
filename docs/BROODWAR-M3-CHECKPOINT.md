# Brood War presentation and commander checkpoint

2026-11-04 UTC. This checkpoint is **partial completion** of the requested
presentation/VR commander work. No final Dots model was accepted or installed.
No assets were added to git. No branch push was performed. Dots' delivery
folder was not modified. The D2R job was active, so port 9946 and the simulator
were left alone; XR was compiled only.

## Delivery intake

`python3 scripts/broodwar-native.py assets-review <dir>` stages a private copy,
checks delivery metadata/geometry/LOD1/texture dimensions/clip mappings, renders
17 measured-camera comparisons and a 36-view perspective close-up, runs the
native rig checks, and invokes the atomic pack installer only on pass.
[DOTS-DELIVERY.md](DOTS-DELIVERY.md) describes the exact contract and limitations.

Final baseline mock report:
`runtime/3d-proof/m3/intake/20261104T074316408270Z/review.json`.
Marine minimum IoU 0.4915 / SSIM 0.0160; SCV 0.6204 / 0.0182. These are unfitted
baseline deliveries with explicit calibrated idle clips, not the prior fitted
candidates. Both failed. Missing source/LOD delivery requirements also reject
them. Per-unit `comparisons/00.png` through `16.png`, `closeup.gif`, `turntable/`
and `native/` contain the proofs. Earlier intake directories are superseded:
their bounds included a non-rendered Blender bone-display helper.

Remaining intake limits: semantic forward orientation and artistic acceptance
need visual inspection; no independent camera calibration for other unit types;
LOD1 is audited/archived but not switched at runtime. The auto-install success
path reuses the tested installer but has no final-quality passing delivery yet.

## Presentation

The owned DAT weapon graphics map exports 122 weapon IDs to 33 usable HD ANIM
image sets. `prepare_effect_sprites.py` writes private PNG frames and provenance
under `runtime/effect-sprites/`. Image 503 is explicitly missing because its ANIM
canvas is zero-sized. Native bullet image/frame controls the billboard frame,
including native impact phases. Authored GLB effect overrides still take priority.
Unsupported images retain the existing procedural fallback, without retaining
a stale previous sprite frame. Muzzle and generic damage pulses remain procedural.

Terrain fog now interpolates shared corner values within explored tiles, with
black boundaries against unexplored space. Selected units have segmented,
health-colored HP bars and cyan shields. Existing selection circles, map-derived
HD terrain and terrain-conforming geometry remain in use. Session changes clear
fog, control groups and sprite counters.

Live native captures before the client restart:
`runtime/3d-proof/m3/live-presentation/live-{0,7,106}.png` and `live-proof.json`.
The first live proof was native frame 111607; the regenerated proof includes
selection bars. `effect-proof/` renders representative frames from every exported
image through SceneKit with synthetic timing; it is not a live combat test.

**Not completed:** death sprite/iscript playback, independent muzzle/impact
iscript effects, creep projection, sprite doodads, measured light-direction
matching, and live verification across all eight tilesets. The existing terrain
reader supports all eight names; that does not prove their visual acceptance.

## Commander and bridge

Desktop controls verified through the actual app:

- Left drag box-selects owned visible units; Shift adds. Option-drag looks.
- Double-click selects the visible same-type group, capped at 12.
- Ctrl+0–9 assigns groups; 0–9 recalls; Shift+number adds.
- V toggles commander height; F12 saves a PNG and selected/group state.

At native frame 118683, box-selection and group 4 recall selected five units.
At frame 118818, double-click selected the two visible SCVs. Captures and JSON
are in `runtime/3d-proof/m3/desktop/`. The height toggle is a desktop commander
view, **not a verified stereo tabletop-scale implementation**.

The original command bridge adds research, upgrade, morph, addon, seven named
mode/ability actions, unload-all, cancellation, rally, repair and load.
[BROODWAR-COMMANDS.md](BROODWAR-COMMANDS.md) defines the fields and limits.

**Not completed:** targeted spells, per-passenger unloading, units.dat/native
button-generated wrist command cards, other-hand minimap/resources, controller
drag/double-tap/groups and stereo scale toggle. Existing XR controls still use
the previous group/action-cycle interaction.

After rebuilding the DLL, only the dedicated Brood War bottle was restarted.
The original client then listened on the bridge but did not advance the frame
hook (menus or paused); the UI tool could not attach to its Wine window. Live
acceptance of new commands and effects is therefore pending. Do not treat queued
ACKs, packet tests or synthetic captures as gameplay success.

## Verification artifacts

Under `runtime/3d-proof/m3/`:

- `tests-all-native.log`: 24 native-tool Python/Swift tests.
- `tests-assets.log`: 17 asset-pack tests.
- `miri.log`: six Rust packet/history tests with strict provenance and alignment.
- `packet-regression-mutations.log`: all four command validation tests fail under
  their targeted mutation; restored Rust tests pass.
- `frame-regression-mutation.log`: removing the crop guard makes the clipping
  regression fail; restored implementation passes.
- `runtime-final.log`: 25 deterministic renderer checks, including session cleanup.
- `build-final.log`, `build-xr.log`, `build-bridge.log`: desktop, XR and DLL builds.
- `mock-review-final.log`, `effect-export.log`, `effect-proof.log`: art evidence.

Miri covers Rust packet/history behavior, not execution inside the closed-source
client. No physical headset or controller acceptance is claimed.
