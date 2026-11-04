# M6 checkpoint

M6 is partially accepted. Live controller transitions and populated command cards
are now demonstrated; exhaustive command parity and Remastered visual parity are
not complete. Private evidence is at
`~/.local/share/soju/broodwar-native/3d-proof/m6/`.

## Committed changes

- `529b1ce`: identify the white object beside the minerals as the type-188
  geyser. Its imported model has materials but no textures. Prefer the owned
  native image chain for this type, restoring the textured geyser.
- `7e70846`: share priced command choices between desktop and wrist controls,
  display hotkeys, disable unaffordable commands, expose technology costs, and
  add patrol, return cargo, larvae selection and lift/land command handling.
- `6f08f1f`: exclude decorative rays, reticles and selection outlines from
  selection hit testing; preserve dragging across transient misses and stabilize
  clicks on animated unit meshes.
- `b264f19`: pad terrain textures with strips and corners from actual neighboring
  map tiles before filtering. This addresses texture-edge sampling; raised cliff
  geometry still needs acceptance.
- `9e7d212`: record selected units and command-card availability in diagnostics.
- `dcb7c46`: offer production cancellation only while production is active.

The simulator source was found at
`~/Projects/soju-vr-lab/oxrsys-winebridge/oxrsys-src`. Local branch
`soju/broodwar-m6-input`, commit `03e0b97`, adds opt-in expiring controller replay
through the simulator's existing tracking sender. Replay traverses the runtime's
OpenXR input path, rather than injecting viewer selections or gesture counters.
Malformed or expired input falls back to ordinary controls. Five pre-existing
modified simulator files were preserved and excluded from this commit.

## Live evidence

Paths below are relative to the M6 evidence directory.

| Acceptance observation | Evidence |
| --- | --- |
| Textured geyser, desktop and XR | `geyser-desktop/frame-00008.png`, `geyser-simulator.png` |
| Desktop cards for SCV, Marine, Probe, Command Center, Hatchery, Nexus and Drone | `desktop-cards/{7,0,64,106,131,154,41}.png` and corresponding JSON |
| XR SCV and Marine cards | `select-37.59-30.12-simulator-final.png`, `select-38-26.18-simulator-final.png` |
| XR Command Center and Hatchery cards | `select-24-16-simulator-final.png`, `select-24-40-simulator-final.png` |
| XR Nexus, Probe and Drone cards | `select-48-40-simulator-final.png`, `select-48-44-simulator-final.png`, `select-22.65-41.37-simulator-final.png` |
| Box-select press, drag and release | `box-held-simulator-final.png`, `box-simulator-final.png`, `box-xr-controller-events.json` |
| Double-tap | `double-simulator-final.png`, `double-xr-controller-events.json` |
| Grip-zoom | `zoom-held-simulator-final.png`, `zoom-simulator-final.png`, `zoom-xr-controller-events.json` |
| Trigger issues native move | `trigger-scene-last-command.json`, `post-gesture-native.json` |
| Wrist Train SCV queues real production | `card-train-native.json`, `card-2-scene-last-command.json` |
| Zero-resource disabled buttons on both surfaces | `low-desktop.png`, `low-xr.png`, `low-resource-result.json` |
| Owned client HD option unavailable | `client-video.png` |

Controller traces show boxSelections increasing to 1, doubleTaps to 1, and grabs
to 1 with zoom changing from 20 to 16.445726. The trigger move was acknowledged at
native frame 6655, destination (889, 977). Train SCV was acknowledged at frame
8387 and appeared as type 7 in the native production queue, with its mineral
cost deducted. On the zero-resource fixture, pressing the disabled train button
submitted no command and the queue stayed empty. The low-resource screenshots
precede the final cancellation-button refinement; final desktop cards include it.
Earlier failed attempts remain in the evidence directory and are not acceptance
proof. `card-2-simulator-final.png` belongs to the later zero-resource run.

## Validation and scores

- Asset-venv Python: **245/245**, `full-suite-final.log`.
- Desktop and XR runtime checks: **52 each**, `runtime-final.log`, `xr-runtime.log`.
- Native Rust tests: **14**, `rust-tests.log`.
- Simulator replay tests: **2**, `simulator-tests.log`; simulator build succeeded.
- Matched owned Remastered client-frame SSIM: **unavailable**. The current owned
  client renders classic graphics and its HD Graphics option is disabled. No tile
  assembly is substituted for the requested reference.
- Brightness/contrast and sun-direction score: **unavailable**. The M5 sun fit
  (R-squared about 0.089) is not a reliable measurement; no calibration is claimed.

## Remaining gaps

1. Cards are populated for all seven requested units, but producer relationships
   and hotkeys still use explicit tables rather than an exhaustive native buttonset.
   Prerequisite-aware availability, some hotkey collisions on flat pages, and
   upgrade pricing remain. Every button has not been exercised live; representative
   move, production and unaffordable-button behavior is verified.
2. Darkness, contrast and sun direction still need a working owned HD client frame
   of the same map area, camera registration and a real SSIM comparison.
3. Neighbor gutters are tested and displayed live, but raised cliff/ramp seams and
   geometric stretching remain unaccepted. There is no quantitative seam score.
4. Other coarse imported models and white material patches remain beyond the
   identified geyser. The geyser fix does not establish model-wide parity.

No game assets were committed, no writes were made to `asset-upgrades/`, and
nothing was pushed. Only the dedicated bottle and viewers were restarted. No
running `devplay.py` was found and D2R was not stopped. The normal terrain fixture
was restored after the zero-resource check.
