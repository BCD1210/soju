# Brood War M17 checkpoint

M17 is **partial; the three-race simulator controller gate has passed**. Started from
`6b7b387` on `feat/broodwar-3d`. Evidence lives outside the repository at
`~/.local/share/soju/broodwar-native/3d-proof/m17/`. No push was performed.

## Controller routing and viewer crash

The M16 log accepted both suggested profiles, attached the commander action set,
and reached session state 5 (focused), but waited for a headset connection. The
simulator retained a disconnected D2R image. Process inspection found no running
D2R game. Its stale stream was disconnected through the simulator UI; no D2R
process, bottle, or release installation was stopped or changed.

Connecting the simulator to the Brood War viewer established the Touch profile
and activated both controllers. That exposed an application crash when the wrist
panels became visible. `wrist-color-crash.ips` identifies
`_NSTaggedPointerColor redComponent` inside `Commander.sendSceneView()`. Textured
panels used a grayscale `.white` fallback, then queried RGB components. The
overlay now returns an explicit white RGBA vector for image contents. The same
unsafe `.black` fallback in streamed material maps now uses an explicit sRGB
color. Regression checks exercise image tint, grayscale alpha conversion, and
missing material maps.

In the initial fixed run, the viewer remained running with `aimValid=1` and
`leftValid=1`, and the simulator displayed Brood War. The simulator process
remained PID 929 with its original start time. Neither simulator nor shared
runtime was rebuilt, restarted, or modified. Only the isolated Brood War
viewer/game was restarted. `build-evidence.json` records binary hashes and
process identity; `xr.log` and `xr-fixed.log` retain the failed and fixed runs.

## XR controller table

The user approved relaunching the private M6 simulator app with its existing
`OXRSYS_SIMULATOR_INPUT_FILE` replay setting. That app was relaunched; shared
OXRSys source/services and the D2R test copy were not touched. Replay packets
expire after two seconds and travel through the simulator tracking sender,
runtime actions, viewer ray picking, wrist card, and original game handlers.
No selections or native orders were injected by the acceptance driver.

| Race | Move | Attack | Build | Train | Spell |
|---|---|---|---|---|---|
| Terran | Marine position changed | Enemy Marine HP 40 → 4 | Supply Depot completed | SCV completed | Defensive Matrix timer activated on target vessel |
| Zerg | Hydralisk position changed | Enemy Marine removed following attack | Spawning Pool completed from selected Drone | Hatchery card → larvae → completed Drone | Dark Swarm effect unit created; caster energy decreased |
| Protoss | Dragoon position changed | Enemy damage observed following attack | Pylon completed | Probe completed | Feedback consumed caster energy and killed enemy High Templar |

Every row passed `verify_chains.py`; `xr-controller-table.json` lists the exact
before/after files and native handles. Each case includes simulator replay poses,
controller edges, card-click counts, selected handles, native acknowledgements,
and native snapshots. The `*-card*.json` files retain the actual card ray hits.
Metal readbacks are in `terran-v3-captures/`, `zerg-captures/`, and
`protoss-captures/`. This is simulator acceptance, not physical-headset testing.

The first attempts exposed a second failure: the 1.5 GiB default texture budget
was exhausted in the full command fixture, freezing scene publication and its
picking data. The rejected run is retained as `residency-failure-*.json`. The
accepted fixture runs explicitly use the existing viewer option
`SOJU_BW_TEXTURE_MIB=4096`; stale packet age and scene-worker errors are checked.
The default-budget failure is not claimed fixed. Unit and card mesh bounds are
now included in diagnostics, allowing the replay driver to aim at rendered
surfaces and move around intervening units without bypassing ray picking.

The scripts also retain failed targeting attempts, a driver collision, and a
movement-wait threshold failure. They are not counted as passes. The accepted
cases were rerun serially; the replay driver now locks its single input file.

## Validation

- Python: **282 passed, 60 subtests passed**, `tests-python.log`.
- Rust: **17 passed**, `tests-rust.log`.
- Runtime: **178 desktop / 183 XR / 183 fallback**, corresponding
  `runtime-*.json` and `runtime-*.log`; all report passed.
- Both desktop and XR viewer builds succeeded. The live simulator connection
  survived the formerly crashing wrist-panel path.

The automated runtime checks are separate from the controller evidence above and do not certify rendered visual acceptance.
No new final stress result is claimed.

## Ordered work still open

1. Controller gate complete: all three races passed the five command chains.
2. Drive cards from native evaluated button sets; reach zero structural
   mismatches over every fixture type and verify shared multi-selection commands.
3. Fix and pass rendered cliff/footprint rejection gates.
4. Rerun the 125-unit stress after the preceding gates pass.

Steps 2–4 remain pending at this controller checkpoint. No game assets or
`tools/broodwar_native/target/` were staged. `asset-upgrades/` was not modified.
