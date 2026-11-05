# Brood War M17 checkpoint

M17 is **complete for the four requested gates**. Started from
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

## Native evaluated action cards

The live desktop and wrist cards now consume `evaluated_buttons`. The bridge
locates the loaded original table, installs the requested live selection only
while calling the original condition functions on the game thread, and restores
the client selection/count and requirement-error state. Callback addresses must
belong to executable code; the selection count is resolved from a validated
original condition and checked against the selection getter. The card uses the
returned hidden/disabled/enabled state, original position, and original hotkey
and label from the owned string table. String data stays in the private runtime.

Worker build buttons open the original basic and advanced submenus. Empty slots
remain empty. Returning from a submenu and changing native button sets reset the
menu. Shared selections follow the original linked/shared-set rules; production
and ability group packets retain the whole applicable selection.

The reference for shared-set rules and condition states is
[the interface button implementation](https://github.com/BoomerangAide/GPTP/blob/ce321f0fa83174aee741b91f1b2eaec30300773e/GPTP/hooks/interface/buttonsets.cpp),
with [the DAT property definitions](https://github.com/BoomerangAide/GPTP/blob/ce321f0fa83174aee741b91f1b2eaec30300773e/GPTP/SCBW/enumerations.h).
The live mixed-Drone/Hydralisk test caught a capability/status flag mix-up before
acceptance; the corrected card uses shared burrow set 247 and the original group
handler burrows both units (`shared-burrow-fixed.json`).

`button-parity.json` compares presentation rows against raw game-evaluated
records and the original string table, and separately checks shared-set choice.
It covers all 76 M16 fixture types with zero structural mismatches across 1,815
samples, including 1,618 mixed/two-unit selections and all six build submenus. Produced
Archon, Interceptor, Scarab, Nuclear Missile, and Infested Command Center states
are included. Negative tests reject omitted/displaced buttons, incorrect
hotkeys, enabled disabled-buttons, and unrestored selection context. This is a
structural gate; it does not substitute for rendered visual acceptance.

Simulator input also exercised the new Zerg submenu through a completed
Spawning Pool (`native-ui-zerg-build-menu.json`,
`native-ui-zerg-built-pool.json`) and Hatchery → select larvae → morph Drone
(`native-ui-zerg-train-complete.json`). One follow-up attempt lost tracking while
builds ran concurrently; after reconnecting the private stream, the training
retry passed. Those failed attempts are retained and are not passes.

## Rendered cliff and contact gates

Terrain height now changes Y only. Removing the height-dependent Z shear keeps
steep descending cliffs from folding across their native map domain. Landed
buildings use a continuous bottom-band adjustment against the sampled terrain;
the upper relief remains unchanged, shared source meshes remain immutable, and
lifting off restores the source geometry. An earlier per-column deformation
visibly stretched side silhouettes and was rejected.

`--rendered-gates` renders through the Metal stereo pipeline and reads back the
left eye. Jobs explicitly pin the owned terrain release, avoiding the currently
loaded flat command fixture. The cliff job uses native VF4 heights; the slope
job uses the same M15 linear slope fixture as the earlier contact investigation.
Command Center, Hatchery, and Nexus are checked from four directions.

The normal cases require visible building pixels, depth-tested opaque-base
contact probes matching ground-reference pixels, complete top-down terrain
coverage, and the expected perspective terrain silhouette. Controls remove the
building, raise or bury it by 0.75 map units, reintroduce the old terrain shear,
or flatten terrain height. Probe comparisons use a two-pixel raster tolerance;
terrain comparisons use one pixel. A flattened slope initially escaped a loose
silhouette threshold; the final threshold is 0.999. A terrain cache bug exposed
by these controls also now invalidates batches when textures or transforms change.

Final release-build evidence: `rendered-cliff/acceptance.json` and
`rendered-slope/acceptance.json`, with colored views, contact readbacks, and
terrain masks beside them. Across both jobs, **16 normal views pass and all 80
negative controls are rejected**. Normal contact and terrain-coverage overlap
are 1.0. `rendered-build.json` records the release binary hash. The earlier debug
runs remain under `rendered-*-debug/` and are not final acceptance evidence.

These gates establish the tested contact/terrain behavior, not hidden-side model
fidelity: native sprite relief still has view-dependent silhouettes and baked
artwork. They are not a claim that every building has a complete solid model.

## Final 125-unit stress

After the preceding gates passed and step 3 was committed, the private simulator
was connected to the population fixture. The run warmed for 30 seconds and ran
the 45-second driver. Its last periodic metrics snapshot covers **44.40 seconds
and 3,997 stereo frames**. All nine native samples retained **125 owned units**;
the final renderer count was 137. All ten group-move submissions addressed 12
Marines and succeeded. This stress load uses native commands, independently of
the simulator-only controller acceptance above.

| Metric | p95 | p99 | Maximum |
|---|---:|---:|---:|
| Frame cadence | 11.86 ms | 12.20 ms | 15.69 ms |
| After-wait host work | 0.76 ms | 1.68 ms | 4.64 ms |
| Stereo GPU | 4.66 ms | 5.74 ms | 6.89 ms |
| Stereo CPU encode | 0.55 ms | 0.61 ms | 0.72 ms |

Estimated missed cadence slots: **0**. After-wait period overruns: **0**. GPU
samples over 11.111 ms: **0**. Cadence samples over 11.111 ms: 2,006, which are
not the missed-slot estimate. M16 cadence p95 was 11.87 ms and GPU p95 was
5.94 ms. These remain host/GPU measurements, not physical display telemetry or
a long-duration stability claim.

The run used the same explicit 4 GiB fixture texture limit as the controller
runs. Renderer-owned texture storage peaked at 1,182,680,192 bytes with zero
rejections. Mid-measurement diagnostics retained valid controller poses and a
178.64 ms scene-packet age. Evidence is in `population/live-final-metrics.json`,
`population/live-state-samples.json`, `population/stress-orders.json`, and
`population/live.log`; `live-launch.json` records the executable hash.

## Validation

- Python: **286 passed, 60 subtests passed**, `tests-python-visual.log`.
- Rust: **17 passed**, `tests-rust-visual.log`.
- Runtime: **186 desktop / 191 XR / 191 fallback**, corresponding
  `runtime-*-final.json` and `runtime-*-final.log`; all report passed.
- Both desktop and XR viewer builds succeeded. The live simulator connection
  survived the formerly crashing wrist-panel path. The final population map adds
  one terrain-dependent check to each earlier 185/190/190 run.

The automated runtime checks are separate from the controller evidence above and do not certify rendered visual acceptance.
The final stress result is recorded above.

## Completed sequence

1. Controller gate complete: all three races passed the five command chains.
2. Native action-card gate complete: 0 structural mismatches across 76 fixture
   types, six build submenus, and shared-selection checks.
3. Rendered gate complete: 16 accepted normal views, 80 rejected controls.
4. Final 125-unit stress complete: cadence p95 11.86 ms, 0 missed slots.

The private simulator was left receiving the live Brood War population session
after measurement (`final-session.json`). Shared OXRSys source/services and the
D2R test copy were untouched.

All four steps were completed in order with local commits. No game assets or
`tools/broodwar_native/target/` were staged. `asset-upgrades/` was not modified.
