# Brood War M17 checkpoint

M17 is **partial; the first ordered acceptance gate remains open**. Started from
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

After rebuilding, the viewer remained running with `aimValid=1` and
`leftValid=1`, and the simulator displayed Brood War. The simulator process
remained PID 929 with its original start time. Neither simulator nor shared
runtime was rebuilt, restarted, or modified. Only the isolated Brood War
viewer/game was restarted. `build-evidence.json` records binary hashes and
process identity; `xr.log` and `xr-fixed.log` retain the failed and fixed runs.

## XR controller table

| Race | Selection and card | Move / attack / build / train / spell |
|---|---|---|
| Terran | Simulator left-trigger press/release selected Marine `7ff000ed40b8-1` and displayed its card in the population map | Not completed |
| Zerg | Not attempted this round | Not completed |
| Protoss | Not attempted this round | Not completed |

`population-xr-controller-events.json` records the fresh trigger edges and
selection at timestamps 1793830821.738 and 1793830821.755. Associated input and
controller diagnostics are saved with the `population-` prefix. This is
selection/card evidence, **not** card-click or native-command acceptance.

The isolated game was subsequently loaded with the existing M16 Terran command
fixture. `terran-tracking-samples.json` records advancing native frames
4066/4091/4116 with both controller poses valid. `terran-state.json` preserves
the native snapshot. Pose positions/orientations and card-click count are now
included in controller diagnostics to distinguish tracking from interaction.

The running simulator has no `OXRSYS_SIMULATOR_INPUT_FILE` environment setting.
The existing M6 expiring replay mechanism therefore cannot be used in that
process. UI keyboard triggers and repeated arrow inputs reached the simulator;
relative mouse attempts did not change controller aim. Reliable full command
chains have not been established through the available UI controls. A request
to allow relaunching only the simulator app with its existing replay setting is
pending. No exception to the restart restriction has been assumed.

## Validation

- Python: **282 passed, 60 subtests passed**, `tests-python.log`.
- Rust: **17 passed**, `tests-rust.log`.
- Runtime: **178 desktop / 183 XR / 183 fallback**, corresponding
  `runtime-*.json` and `runtime-*.log`; all report passed.
- Both desktop and XR viewer builds succeeded. The live simulator connection
  survived the formerly crashing wrist-panel path.

These checks do not certify the command matrix or rendered visual acceptance.
No new final stress result is claimed.

## Ordered work still open

1. Complete all three races' controller-only selection/card/command chains.
2. Drive cards from native evaluated button sets; reach zero structural
   mismatches over every fixture type and verify shared multi-selection commands.
3. Fix and pass rendered cliff/footprint rejection gates.
4. Rerun the 125-unit stress after the preceding gates pass.

Steps 2–4 were not started, preserving the requested order. No game assets or
`tools/broodwar_native/target/` were staged. `asset-upgrades/` was not modified.
