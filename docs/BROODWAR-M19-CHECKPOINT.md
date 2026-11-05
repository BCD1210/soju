# Brood War M19 checkpoint

M19 and the remaining M18 XR gates are **not qualified**. Work starts at
`da30dbb` on `feat/broodwar-3d`. Private evidence is under
`~/.local/share/soju/broodwar-native/3d-proof/m19/`.

Latest results (`b42d576`, 2026-11-05 America/Los_Angeles): three-race XR
controller chains, table/ground/air possession, manual/death exits and combat
captures are verified. The default ten-minute duel passes sustained combat,
residency and the 70 ms XR display-estimate gate. All three 125-unit XR views
pass cadence and residency. **Open performance gates:** possession latency
76.45 ms, desktop actual presentation latency 78.45 ms, and ten-minute duel
cadence 12.40 ms. These exceed 70 ms / 70 ms / 12 ms respectively. No overall
qualification is claimed. Physical-headset presentation remains unverified.

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
- Initial implementation runtime suites: **209 desktop / 214 offline XR / 214 fallback**
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

## Direct native publication

Native snapshots now enter the scene worker directly from the bridge queue.
The main owner still receives every ordered control/card update before admitting
the next bridge read. The worker accumulates native fog and creep regions itself,
including session resets; runtime checks verify region coordinates, outside-region
retention and reset behavior. Desktop/XR/fallback checks pass 234/239/239.
`xr-live/build-direct-publication-evidence.json` records matching staged sources.

`stress-desktop-direct/` measured **87.92 ms native-to-presented p95** and 33.83 ms
packet age p95 over 44.99 seconds. This desktop uses compositor presentation times
and runs at approximately 50 Hz; its intervals are not a 90 Hz XR cadence gate.
`stress-tabletop-direct/` measured **87.12 ms native-to-display estimate p95**,
34.80 ms packet age p95, **11.91 ms cadence p95, zero missed slots and zero
rejections** over 45.30 seconds. Neither latency result meets 70 ms. CPU samples
were collected after the metric windows, and are excluded from qualification.

## Packet preparation costs

The worker now stores typed projected bounds and serializes pick/capture
metadata when a diagnostic consumer requests it. Cache color keys avoid array
debug formatting. Native state comparison reuses decoded dictionaries while
still comparing every field; independently decoded snapshots measured about
5.6 ms versus 1.63 ms per comparison with identical dirty sets. Streamed unit
layers construct the known observer-facing frame directly, avoiding an
immediate second geometry replacement. Runtime checks verify JSON metadata,
nonvisual state changes, tint-key separation and observer-frame construction.
Desktop/XR/fallback runtime checks pass **241/246/246**.

`stress-desktop-packet-cost/` reports **78.38 ms native-to-presented p95** over
45.03 seconds, using default settings and actual compositor timestamps.
This improves the original 97.63 ms result but still fails the 70 ms target.
`xr-live/build-packet-cost-evidence.json` records binary and source hashes.
The intermediate `stress-desktop-deferred/` experiment and all post-measurement
CPU samples remain available; neither is substituted for final qualification.

## Reuse during terrain refinement

The worker can retain unit batches when geometry, pose, visibility and membership
are unchanged. Projected bounds reuse also accounts for the camera projection and
viewport. Movement, viewport resizing, hiding and removal invalidate the relevant
values; retained packets remain immutable. Desktop/XR/fallback runtime checks
pass 247/252/252. Live performance is still subject to remeasurement.

The `691d794` XR stress runs now meet 70 ms in table and ground views:
`stress-tabletop-unit-reuse/` measured **68.12 ms** latency p95, 11.89 ms cadence,
zero missed slots; `stress-ground-unit-reuse/` measured **68.03 ms**, 11.93 ms,
zero missed slots. Possession remains open: `stress-possess-unit-reuse/` measured
**116.51 ms** latency p95, 11.92 ms cadence and zero missed slots. All use the
same default-settings binary and preserve native move acknowledgements and
actual movement. These are XR display estimates, not physical presentation.

Desktop refresh-rate/drawable-count experiments did not improve the qualified
latency and were removed. Their `stress-desktop-display-refresh/`,
`stress-desktop-double-buffer/` and `stress-desktop-two-drawables/` artifacts remain.
The display reports a 100 Hz mode; requesting 90 fps yields approximately 50 fps
in the existing desktop view. Changing that request alone increased queued
presentation delay, so no such change is retained.

## Native update scheduling

Bridge admission now starts the next read as soon as the worker accepts the
previous snapshot. A queued view-only publication may yield to a pending native
update once; consecutive deferrals are prohibited so terrain work still runs.
Ordered native updates and event deltas remain intact. Desktop/XR/fallback
runtime checks pass 247/252/252.

`stress-possess-native-priority/` improved XR latency p95 to **83.06 ms** over
43.86 seconds, with **11.91 ms cadence p95, zero missed slots, zero rejections**
and 35.14 ms packet age p95. It still fails 70 ms. Its moving-camera CPU sample
was collected only after freezing `live-final-metrics.json`.
`stress-desktop-native-priority/` measured **78.55 ms actual presentation p95**
over 45.05 seconds; the desktop latency gate also remains open.

## Contact and streamed-layer preparation

Immutable terrain corner heights now avoid repeated four-height averaging.
The contact interpolation grid additionally retains the exact sampled values
used by the original triangular interpolation. Runtime checks compare slopes,
triangle boundaries and outside-map coordinates. On the 128×112 map, the grid
uses about 28 MiB of CPU memory per terrain instance; the 1,536 MiB texture
budget is unchanged. Sprite layers retain geometry when their displayed image,
frame, team color and draw function are unchanged. Relief construction no longer
allocates a discarded plane. Streamed Metal materials share read-only frame
properties; fallback materials retain independent mutable properties.

Runtime checks on `b42d576` pass **251 desktop / 256 XR / 256 fallback**
(`xr-live/runtime-shared-material-*.json`). Python and Rust rechecks pass
**286 tests + 60 subtests / 18 tests** (`xr-live/final-python.log`,
`xr-live/final-rust.log`).

| Possession run | XR latency p95 | Cadence p95 | Missed slots |
|---|---:|---:|---:|
| `stress-possess-layer-reuse/` | 78.21 ms | 11.91 ms | 0 |
| `stress-possess-contact-grid/` | 75.24 ms | 11.89 ms | 0 |
| `stress-possess-shared-material/` | 76.45 ms | 11.87 ms | 0 |

All three retained implementations preserved native movement acknowledgements,
actual movement, tracking freshness and zero texture rejections. The final
shared-material run measured 41.56 seconds and packet age p95 35.01 ms.
Possession still fails the 70 ms latency gate. XR latency remains a display
estimate; compositor presentation timestamps are unavailable in this runtime.

Two experiments were removed: merging pending terrain refinement into native
publication measured 86.20 ms, and a ground-pose cache measured 81.22 ms with
one missed slot. Their run directories remain under the M19 proof root; patches are in `xr-live/`.
Earlier successful runs are not substituted for the final binary's measurements.

## Optimized default ten-minute duel

`duel-xr-optimized/qualification.json` verifies **613.89 measured
seconds**, 609.72 seconds between native samples, and
14,516 original frames advanced on `b42d576`. The run recorded
**217 attacks, 194 damage events and 24 deaths**. Every sampled scene and tracking
state stayed fresh and the owned army survived. Defaults were unchanged, captures
were disabled, peak texture residency was **1309.58 MiB**, and there were
**zero rejections and zero missed slots**.

Latency p95 improved to **68.30 ms**, passing the
70 ms XR display-estimate threshold for this duel. Packet age p95 was
**40.59 ms**. Cadence p95 was **12.40 ms**, which
still fails the 12 ms limit. The sustained combat/residency gate passes; the
combined performance gate does not. Post-`xrWaitFrame` work p95 was
0.78 ms and wait duration p95 was
11.70 ms. These durations identify where the
measured time is spent, without establishing the cause of scheduling variation.
Shared runtime source and services were not changed.

## Final same-binary stress and evidence

`xr-live/xr-stress-final.json` verifies matching XR binary hashes, 125 owned
units, native move acknowledgements, actual movement, fresh scene/tracking
samples, default settings, disabled captures and zero texture rejections.

| XR view | Measured seconds | Cadence p95 | Missed slots | Packet age p95 | Display-estimate p95 |
|---|---:|---:|---:|---:|---:|
| Table | 45.35 | 11.91 ms | 0 | 35.16 ms | 63.27 ms |
| Ground | 42.96 | 11.92 ms | 0 | 34.67 ms | 64.91 ms |
| Possession | 41.56 | 11.87 ms | 0 | 35.01 ms | 76.45 ms |

`stress-desktop-final/` measured **78.45 ms actual compositor presentation p95**
over 45.05 seconds, packet age p95 **33.35 ms**, and zero texture rejections.
The original 97.63 ms desktop baseline improved, but 70 ms remains unmet.
Desktop's approximately 50 Hz intervals are not a 90 Hz XR cadence result.

`xr-live/build-final-evidence.json` records both binary hashes and zero staged
Swift-source mismatches. Final Python/Rust checks passed again after the last
source commit. Saved three-race controller/mode evidence was revalidated by
`chains-verification-final.log` and `modes-verification-final.log`; those live
functional sessions predate the packet optimizations, as identified by their
launch evidence. The same-binary stress and duel results above use `b42d576`.

`xr-live/user-media-verification.json` verifies all six named screenshots against
their raw-source hashes and the **13.6-second** first-person combat video. They
remain in `user-shots/`; no game imagery, video, native assets or build output is
tracked. Work is committed locally without pushing. The existing model-queue
edit belongs to the separate asset work and remains untouched.
