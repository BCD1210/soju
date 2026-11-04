# Brood War M4 checkpoint

2026-11-04. This is partial completion: live gameplay and presentation have
substantial new evidence, but controller-gesture acceptance and matched lighting
remain open. No assets were added to git, no push was performed, and
`asset-upgrades/` was not modified. Only the dedicated Brood War bottle/viewers
were restarted. `pgrep -f devplay.py` found no running D2R job before XR use.

All evidence below is private, relative to
`~/.local/share/soju/broodwar-native/3d-proof/m4/`.

## Reliable private gameplay

The original frame-0 block was the StarCraft Tips modal (`startup.png`). The
legacy registry preference did not suppress it; isolated SCR
`documents/StarCraft/CSettings.json` with `"tip": 0` does. The launcher preserves
other settings and requires two advancing native snapshots of the requested map
before reporting `running: true`, with a 120-second timeout. No UI attachment is
needed. Two initial restarts passed in `startup-restart-{1,2}.log`.

The first fixture inherited a randomized force shared by human and computer
players, causing run-dependent player-slot assignment. The v2 generator uses
human 0, enemy 1, separate forces, and no random starts. These follow
[Blizzard's documented force rules](https://classic.battle.net/scc/faq/other.shtml).
All eight corrected tileset restarts report player 0 and 50,000 resources;
`tilesets/results.json` records them. Earlier failed fixtures/captures remain in
`fixtures-player6/`, `tilesets-initial-position-failure/`, and
`tilesets-player0-failure/`.

## Live bridge acceptance

`commands/results.json` and `live-extended-v2.log` verify siege/unsiege,
burrow/unburrow, cloak/decloak, stim, transport load/unload, train queue/cancel/
completion, research and upgrade start/refund/cancel, and larva morph/cancel.
Each check observes native state, not just a queued ACK. Original basic gather,
SCV training, depot completion and follow passed in `live-basic.log`.

`production/results.json` verifies construction start/cancel, addon start/cancel,
rally point state, a newly trained SCV arriving there, unit-target rally state,
and repair increasing HP. Placement failures are retained separately; the final
clear-site run is `live-production-final.log`. `production/desktop-xr-final.png`
and renderer snapshots record concurrent viewers.

Rally required a real bridge fix: issuing unit orders 39/40 changed the building
order without setting its production rally. It now selects the owned building
and sends the original SCR extended targeted-order packet 0x61. Owned buildings
expose `rally: {x,y,target}`; nonowned units and the incompatible Pylon union do
not expose it. References and Rust/Miri evidence are in `native-bridge/rally-fix.md`.
Targeted spells and passenger-specific unload remain unsupported.

## Native presentation

The bridge exposes 228 live units.dat cost/property rows, visible native creep,
dying-unit image chains, and independent sprite chains. Independent SCR sprite
positions must use Samase's getter: the struct's opaque position words produced
invalid coordinates. The corrected getter matches all 24 fixture doodads exactly.
`native-bridge/position-live.json` and `tilesets/results.json` retain evidence.

`prepare_world_sprites.py` exports owned diffuse atlases and frame metadata to
private `world-sprites/`: 989 image IDs backed by 858 atlases. Missing image IDs
are explicit. Native frame numbers drive playback; no guessed iscript durations
are substituted. Authored 3D death clips take priority; otherwise dying-unit and
independent native image chains provide the fallback. Image offsets, mirrored
frames, ground anchors and native shadow draw mode are handled. There is no
claim of exact perspective reconstruction from these 2D billboards.

Creep uses the current-visible native mask and owned Remastered creep tiles.
Zero/unused CV5 variants are excluded. Boundaries are still tile-aligned rather
than a matched native edge transition. All eight tilesets have desktop
`overview.png`, actual OpenXR eye PNG/JSON sequences, and simulator captures under
`tilesets/<name>/`. `tilesets/desktop-contact.png` compares them. Each fixture
contains three CV5-selected sprite doodads; this samples every tileset, not every
possible doodad asset. The Badlands capture predates the later ground-anchor
adjustment; the other seven use that adjustment.

Lighting is measured but **not matched**. `lighting-fit.json` uses 154,282 owned
normal/albedo/prelit samples; R²=0.0886 is too weak to accept a sun-direction fit.
`shadow-edges.json` measures native drawfunc-10 shadow alpha transitions: median
10–90% widths are 0.5, 0.5, 3.25 and 0.75 game pixels for images 240, 248, 277 and
280. These texture-space widths are not a calibrated SceneKit shadow radius.
Existing lighting presets remain; matched ground-reference renders are needed.

## Wrist UI and tabletop

Both wrists, targeting ray, native resources/supply, explored minimap and
contextual command cards render in the simulator. Unit names and live units.dat
prices populate build/train/morph/addon entries. Producer/research/upgrade links
are explicit commander tables; units.dat does not contain complete command-card
relationships. The original engine enforces prerequisites.

Implemented controls: left-trigger drag selection, release click/double-tap,
right-trigger card/target confirmation, left grip scale toggle, right grip table
grab, and two-grip distance zoom. Tabletop scaling affects stereo pose offsets,
IPD, panels, clip planes and camera distance; zoom has a centered-head regression
check. Selection/session changes cancel pending targets. The old HUD was moved
above the cards after live overlap inspection.

The shared SCNView initially caused an XR depth regression (HUD only). Detaching
its scene and displaying the resolved eye texture restored world rendering.
Desktop and XR builds pass. `production/xr-final/` shows the SCV card with live
400/0, 100/0 and 100/0 build prices and the other wrist's resources/minimap.

**Gesture acceptance remains unverified.** Simulator Tab/Space/V/F attempts,
mouse drags and explicit app activation did not produce reliable trigger/grip
transitions in the native input diagnostics. Card clicks, box-selection and
double-tap counters therefore cannot be reported as passed. No physical headset
acceptance is claimed. Simulator scenes ran roughly 24–60 fps; this is not a
72/90 Hz performance certification.

## Demo and validation

`demo_session.py` drives a fresh private fixture for 120 seconds: Terran depot,
Zerg creep colony, training/morphing, attack movement and native combat/deaths.
The final run is under `demo-final/`; `session.json` contains wall-clock bounds,
command ACKs and observed native events, with one native snapshot per second.
Actual resolved OpenXR eye captures are PNG+JSON, not offline rerenders.
`demo-final/frames/` contains the 416 captures within the 120.04-second script
interval (four requested per second; 119.73-second first-to-last span, maximum
0.71-second gap). `verification.json` records native frames 2952 → 5785,
101 attacks, 96 damage events and 10 deaths, completed depots 1 → 2 and creep
colonies 0 → 1, six dying-unit image observations and 690 independent-sprite
image observations. Counters count rendered image observations, not unique deaths.
`combat-eye.png` is an unchanged copy of a recorded eye frame.
The first run remains under `demo/` and is superseded because it predates the
terrain-coverage and independent-sprite-position fixes.

Validation: 24 Python/Swift wire/asset-conversion tests using `asset-venv`;
17 asset-pack tests; 34 deterministic renderer tests; 11 Rust tests and Miri.
Logs/reports are `python-tests.log`, `asset-tests.log`, `runtime-checks.json`,
and `native-bridge/rally-{tests,miri,build}.log`. Miri does not execute the
closed-source game. Live acceptance above covers the foreign runtime separately.

Remaining gaps: simulator controller gestures; targeted spells and individual
unload; complete command-card/technology coverage and prerequisite availability;
matched sun/shadow calibration; native-shaped creep edges; physical headset and
frame-time acceptance. Existing models for types 114, 116, 123 and 135 are
unavailable, and some installed meshes/materials still show white patches.
Dots' assets were not changed or accepted by this work.
