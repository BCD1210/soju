# Brood War M19 checkpoint

M19 and the remaining M18 XR gates are **not qualified**. Work starts at
`da30dbb` on `feat/broodwar-3d`. Private evidence is under
`~/.local/share/soju/broodwar-native/3d-proof/m19/`.

## Implementation

The table now frames the full map at an initial width of 1.8 metres. Camera and
controller rays use the same uniform world scale and heading, with no artificial
head pitch. Native terrain queries accept bounded regions up to 256×256, retaining
per-player fog and visible-creep filtering. Scale, height, rotation and recenter
controls are documented in BROODWAR-VR.md. The ground-camera bookmark survives
table adjustments and is restored when leaving the table.

Possession validates ownership and visible, living, unburrowed, untransported
nonbuilding status. Ground and air viewpoints use the model's height/elevation
metadata where present. Position follows exponentially; subsequent native turns
do not rotate the camera. Movement and ray attacks use original bridge handlers.
Cards continue to use the original handlers and take precedence for pending
targets. The possessed model is omitted from Metal rendering. A lost, dying,
burrowed, transported or no-longer-owned unit returns to the previous view.
Metal output fades in after switching and adds a peripheral motion vignette.

## Validation and desktop evidence

- Python: 286 passed, 60 subtests passed (`tests-python.log`).
- Rust: 18 passed, including full-map terrain bounds (`tests-rust.log`).
- Final-source runtime suites: **209 desktop / 214 offline XR / 214 fallback**
  checks passed (`runtime-*.json`). Both viewer builds and the native module build
  succeeded. `build-evidence.json` records hashes and zero staged-source mismatches.
- Live Terran desktop table captures exist in `desktop-terran/`. The full map and
  original fog were visible. Keyboard zoom and Medic selection at table scale
  were exercised; selecting the Medic exposed its native action card.
- Desktop retest entered a Medic, Science Vessel and Wraith. The Medic's native
  card moved it from (1344,1696) to (1328,1626). A possessed Wraith ray command
  issued attack-move and reached (1245,1709). Escape restored the adjusted table.
  Native/viewer snapshots are `desktop-possession-*.json`,
  `desktop-air-possession.json` and `desktop-air-attack.json`; captures are in
  `desktop-terran-retest/` and `desktop-manual/`.
- Table box selection selected five units (`desktop-box-select.json`).
- Brief automated W key taps did not produce a move order; sustained WASD input
  remains unverified. Card movement is not a substitute for stick movement.
- Initial pointer-only Tab entry did not activate. The retest used the added
  selected-unit fallback and explicit Tab key handling.
- An earlier runtime attempt failed the saved-ground-position assertion; the
  camera-map accessor was corrected and the subsequent desktop suite passed.
- A later final-source runtime attempt trapped in terrain iteration after a
  session reset retained a table-camera position outside the map. The reset now
  restores the ground bookmark before clearing table state, and terrain loops
  clamp both endpoints. `runtime-desktop-crash.ips` retains the failed attempt;
  all three final-source runtime suites passed after this fix.

- Zerg desktop: Hydralisk possession and ray attack-move, Overlord air
  possession, and native Burrow causing automatic return to table. Evidence:
  `desktop-zerg-ground-attack.json`, `desktop-zerg-air.json`,
  `desktop-zerg-burrow-exit.json`, and `desktop-zerg/` captures.
- Protoss desktop: Arbiter possession and attack-move reached (1022,1856).
  Dark Archon ground possession dispatched a native card move; it displaced from
  (704,1696) to (764,1556), without reaching the requested (736,1746). This is
  displacement evidence, not a completed destination check. Escape and V
  restored ground view at (13,1.65,21). Evidence: `desktop-protoss-*.json` and
  `desktop-protoss/` captures.

## Desktop timing observations

These sessions mix table and possession views, include captures and overlap
builds/runtime checks. They are **not isolated 125-unit stress runs** and do not
establish causation for timing differences. All use the default 1,536 MiB budget.

| Race | Seconds | Cadence p95 ms | Missed slots | Packet age p95 ms | Native-to-presented p95 ms | Peak MiB | Rejections |
|---|---:|---:|---:|---:|---:|---:|---:|
| Terran | 288.76 | 22.12 | 14,655 | 51.39 | 96.59 | 675.71 | 0 |
| Zerg | 203.78 | 27.81 | 10,184 | 82.98 | 364.29 | 561.33 | 0 |
| Protoss | 114.24 | 27.18 | 6,139 | 143.03 | 343.78 | 679.05 | 0 |

The cadence and ≤70 ms native-to-presented targets are **not met**.
`desktop-summary.json` records the observations. No latency improvement is claimed.

## Outstanding gates

Remaining desktop command coverage includes sustained WASD, targeting a unit,
build placement and live transport exits. XR per-race controller chains, view
switching, ground/air possession, stick movement, ray attack, manual exit and
live death exit now have evidence below. Each view still needs a passing
125-unit stress run with cadence p95 ≤12 ms, zero missed slots, packet-age and
native-to-presented distributions. M18 still needs its default-settings ten-minute
XR duel and native-to-presented p95 ≤70 ms.

The D2R job finished before the XR continuation. Only the private Brood War
bottle, viewer and simulator were restarted. Shared OXRSys sources and services
remain unchanged. Desktop evidence is not substituted for XR acceptance.
The SceneKit fallback's comfort effect and headset comfort remain unverified.
No new minimap UI is introduced; saved ground coordinates remain stable in table
view. Game assets and build outputs remain private and untracked.

## Commits

- `c3f3ee2`: full-map command table, possession, native terrain bounds and tests.
- The documentation checkpoint is a separate local commit. No push was performed.

## XR continuation: first repair step

The simulator became available for this continuation. A private simulator copy
under `m19/simulator-source/` adds replay head orientation and both thumbsticks;
shared OXRSys sources and services remain unchanged. Controller inputs traverse
its tracking transport and the viewer's OpenXR actions.

Live switching exposed a terrain range trap when a camera lies beyond the upper
map edge. Both terrain range endpoints now clamp to the map. Ground-targeted
cards also exclude intervening unit meshes. Worker-owned wrist overlay nodes
retain their geometry/materials when only the tracked transform changes, avoiding
repeated image conversions. Runtime checks cover these three changes.

Validation in `xr-live/`: 286 Python tests plus 60 subtests, 18 Rust tests,
217 desktop / 222 XR / 222 SceneKit fallback runtime checks passed. The earlier
trap is retained in `table-switch-crash.ips`. Subsequent controller trials reached
ground/table switching, Marine possession and stick movement, but did not finish:
the simulator connection lost tracking during later input. These partial trials
and mixed timing samples are not acceptance results. Tracking diagnostics and
fresh connected runs are required before closing the gates below.

## XR controller qualification

`xr-live/xr-controller-table.json` verifies all three races through simulator
controller input and original native handlers: select, card move, attack with
measured enemy damage, completed construction, completed production, and spell.
Terran completed a Supply Depot and SCV and applied Defensive Matrix. Zerg
completed a Spawning Pool and Drone morph and cast Dark Swarm. Protoss completed
a Pylon and new Probe and cast Feedback. The original Probe subsequently died;
`protoss_train_identity.py` verifies the new unit identity instead of requiring a
net population increase. The earlier count-based timeout is retained.

`xr-live/xr-view-modes.json` verifies ground/table switching and ground/air
possession for Marine/Wraith, Hydralisk/Mutalisk and Dragoon/Arbiter. Each possessed
unit moved through left-stick input, issued a successful ray attack-move, retained
its camera heading while moving, and returned to the adjusted table. Native
movement distances were 158–179 pixels. The replay traces, native acknowledgments,
controller edges, scene snapshots and viewer captures remain under `xr-live/`.
A long selection hold missed an animated Dragoon; a short trigger tap succeeded.
The private simulator was reconnected after the initial tracking loss; these
successful checks use the subsequent connected sessions.

These results close the M18 per-race controller chains and the listed M19 view
controls. They do not yet qualify live death exits, performance, the ten-minute
XR duel, or the requested combat deliverables. No timing pass is inferred from
capture-enabled controller sessions.

## XR combat and death exits

`xr-live/xr-combat-death-exits.json` verifies a possessed Marine, Zealot and
Mutalisk each issuing native attacks, dying through original combat rules and
returning automatically to the table. Native death / observed table-return frames
were 1641 / 1668, 1650 / 1674 and 1564 / 1588 respectively. These are observed
snapshot frames, not an exact transition-latency measurement.

`user-shots/` contains six named, unretouched XR screenshots: Marine firing at
Hydralisks, Zealot in melee, Mutalisk attacking Marines, and table views from the
same three fights while other friendly combatants remained alive. The silent
`marine-first-person-combat.mp4` is 13.6 seconds of possession, approach and
combat. It preserves capture timing from XR frame deltas at 90 Hz, without frame
interpolation. Per-image provenance files identify original frames and hashes;
raw eye images, metadata and native event histories remain in each race folder.
Neighboring air units and close-range effects sometimes occlude the possessed
view; these captures are not a visual-polish acceptance claim.

The capture fixtures only arrange armies on owned terrain; original unit stats
and combat handlers remain in force. Simulator controls enter possession.
Capture staging uses original attack-move handlers for the armies; separate
controller qualification establishes ray/stick orders. Capture-enabled timing
is excluded from performance qualification.

## Panel cache step

`40a67a0` retains immutable panel-image conversions across pose-only captures and
rasterizes minimap fog once per change instead of filling thousands of rectangles.
`runtime-panel-cache-*.json` passed all desktop, offline XR and fallback checks;
Python remained 286 passed plus 60 subtests. Live combat and death-exit checks
also exercised the cache. This is not a latency-target pass. An additional view
scheduling experiment failed its timing comparison and was removed.

## Default XR duel: residency failure and repair

`duel-xr-default-fresh/` completed 606.53 seconds of native gameplay samples
(608.81 seconds in the metrics window), with 173 attacks, 147 damage events and
19 deaths. Tracking stayed valid, but the renderer exhausted its default texture
budget and stopped advancing scene packets. **This run fails qualification**;
its low frame-loop intervals cannot qualify a frozen scene. The earlier
`duel-xr-default/` attempt also remains invalid because the native match advanced
during a viewer rebuild before the strategy started. All attempts are retained.

The expanded native snapshot exposed duplicate terrain storage: scene materials
pinned upload textures after their pixels had been copied into terrain arrays.
The worker now binds immutable array-slice views to those materials. At high
residency, completed upload batches release replaced source ownership through
SceneKit's main-thread flush, temporarily releasing the construction lock.
The render thread continues using immutable packets. Snapshot-only batch inputs
retain their existing ownership behavior. No budget or image-resolution setting
was changed.

The first repair attempts still exceeded the budget during a large initial
reveal. `expanded-map-retest-v6/qualification.json` now verifies all **3,753**
explored tiles, **1,371.53 MiB peak**, and **zero rejections** at the default
1,536 MiB limit. Its captured image was inspected. This frozen snapshot is a
memory and rendering regression check; it is not live latency or duel evidence.
The next live drivers also reject stale scene packets and any residency rejection,
in addition to validating tracking.

The same change set avoids rebuilding native image geometry when only map
coordinates change, and reuses fog/creep diagnostic snapshots until their content
changes. Runtime checks verify pose changes, retained geometry, image-change
invalidation, immutable diagnostics, terrain slice ownership and page reuse.
Live latency and cadence acceptance remain open.

## Default XR stress after terrain ownership repair

`xr-live/xr-stress-release.json` verifies 125 owned units, original move-order
acknowledgements and actual movement, fresh scene packets, and valid simulator
tracking throughout each capture-disabled run on `127892e`. The private simulator
uses its Release build; the earlier Debug tracking-loss attempt remains invalid.
All viewer settings are default, including the 1,536 MiB texture budget.

| View | Measured seconds | Cadence p95 ms | Missed slots | Packet age p95 ms | Native-to-display estimate p95 ms | Peak MiB | Rejections |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Table | 45.31 | 11.92 | 0 | 34.87 | 93.60 | 1173.52 | 0 |
| Ground | 42.07 | 11.88 | 0 | 36.20 | 86.68 | 1075.92 | 0 |
| Possess | 43.63 | 11.93 | 0 | 34.29 | 147.46 | 1074.95 | 0 |

The cadence and residency gates pass in every view. The 70 ms latency gate **does
not pass**. XR reports the later of GPU readiness and runtime predicted display;
it has no physical presentation timestamp. These estimates must not be relabeled
as desktop compositor `presentedTime` measurements. Each private run retains its
exact launch, binary hash, native samples, original orders and metrics.

## Live reveal pressure follow-up

`duel-xr-default-release/qualification-failure.json` records a second failed duel:
318.86 measured seconds, 1,535.95 MiB peak and 124 residency rejections before the
fresh-packet guard stopped it. The live reveal retained terrain and panel caches
alongside terrain arrays and animated units; the earlier frozen-map pass did not
cover that combined working set. This is not a ten-minute qualification.

Under texture pressure, the worker now evicts least-recent unused cache entries
before and after terrain construction. Active packet references remain valid.
The decoder also trims its own cached materials and maps under pressure. These
changes preserve the default budget, texture resolution and original rules.
Runtime checks cover retained resources, eviction accounting and cache reuse.
Python passes 286 tests plus 60 subtests; Rust passes 18; desktop/XR/fallback
runtime checks pass 231/236/236. Build hashes and source matches are recorded in
`xr-live/build-pressure-cache-evidence.json`. Sustained live qualification is
still required.

`expanded-map-pressure-cache/qualification.json` passes all 3,753 explored tiles
with zero rejections, 1,349.78 MiB peak and 1,221.02 MiB retained at the end of the
frozen-map run. It remains a memory regression check, not live duel evidence.

## Completed default ten-minute XR duel

`duel-xr-pressure-cache/qualification.json` verifies the sustained combat gate on
`0567e10`: **608.98 seconds** of native samples, 614.02 seconds of metrics,
14,499 native frames advanced, 272 attacks, 240 damage events and 31 deaths.
The owned army remained alive, tracking stayed valid, and every sampled scene
packet stayed fresh. Default settings, no captures, **1,357.61 MiB peak and zero
rejections**. This closes the sustained default-settings duel functionality and
residency gate.

Performance remains separate: cadence p95 **12.48 ms**, **zero missed slots**,
packet age p95 **40.12 ms**, native-to-display estimate p95 **130.73 ms**. The
12 ms cadence and 70 ms latency thresholds do not pass for this long run. The
three-view 125-unit stress results above remain separate measurements.
