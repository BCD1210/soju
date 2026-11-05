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

Remaining desktop command coverage (including sustained WASD, targeting a unit,
build placement and live death/transport exits),
all three races' simulator controller chains, ground/air possession, movement,
attack, exit, death exit and captures remain to be completed. Each view needs a
125-unit stress run with cadence p95 ≤12 ms, zero missed slots, packet-age and
native-to-presented distributions. M18 still needs its default-settings XR stress
and ten-minute active duel, and native-to-presented p95 ≤70 ms.

The D2R loader and its private OXRSys simulator were active during this work.
No Brood War XR session was started, and shared runtime services/source were not
modified or restarted. Desktop evidence is not substituted for XR acceptance.
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
