# Brood War M15 checkpoint

M15 is partially implemented; the fully playable and visual acceptance gates are **not closed**. Work starts at `796915c` on `feat/broodwar-3d`. Changes are committed locally; nothing was pushed.

## Native commands and selected action cards

Transport loading now passes through the original targeted-order handler. Native snapshots expose research/upgrade progress and levels, cargo counts, addon attachment, larva parent handles and placement footprints. The selected card queries native DAT prerequisite expressions and current upgrade costs, then applies resources, supply, energy and cooldown checks. Unknown requirement instructions fail closed. Original game command handlers remain authoritative.

Larva selection uses the native parent handle. Desktop rally picking supports unit targets. Addons use the original fixed attachment offset; landing/build placement uses the actual footprint. Escape chooses the proper cancellation. In particular, a building morph retains its original type with order 45 and a morph target in its queue; it must not offer train/construction cancellation.

Fresh private fixtures produced 175 successful native observations: 67 Terran, 61 Zerg and 47 Protoss. These counts include setup/start/position checks, not 175 distinct player commands. Coverage includes 17 Zerg morphs, six completed Terran addons, 24 research entries, 50 upgrade entries, 24 spell actions, lift/land, mode toggles, transport load/unload-all, ground/unit rally and cancellation. Research and upgrades were checked through native researched/level transitions; upgrade testing covers the first level, not every level-three prerequisite. Spell checks generally establish native energy spending; selected checks additionally establish status effects, consumption or spawned mines. They do not certify every effect's complete duration or area semantics.

All morph completions include Lurker, Guardian, Devourer, Lair, Hive, Greater Spire, Sunken and Spore. Addon evidence contains an attached, completed unit of the expected type. Transport evidence shows cargo count changes, including the Overlord with Ventral Sacs. Cancellation evidence shows native queue/state removal and refunds where applicable. A queued ACK alone is never a passing observation.

The desktop UI was exercised live: Hatchery → Select larvae → D hotkey → native completed Drone. Evidence: `desktop-action-ui.json`, `desktop-before.json`, `desktop-hotkey-completed.json`. The action card changed to the Drone worker card. XR uses the shared choices and existing controller dispatch, but direct controller end-to-end acceptance was not performed.

## Private fixtures and evidence

`runtime/` below means `~/.local/share/soju/broodwar-native/`. All maps, game images, snapshots and captures remain outside git under `runtime/3d-proof/m15/`.

`make_command_fixture.py` creates owned-source scenarios with original unit/building durations and native costs. Research/upgrade durations are shortened to 24 native frames; the researched gameplay fixtures enable the required technologies and Overlord transport. Separate unresearched fixtures verify research transitions. Provenance files record hashes and these modifications. Earlier invalid or unsuccessful fixture attempts remain in the private evidence directory; the table selects successful later observations without deleting failed attempts. The `v4` fixtures correct energy-valid flags, upgrade limits and UPGx padding.

The fixture layouts follow the [CHK unit record](https://docs.scmjs.dev/chk/unit/) and [container section sizes](https://docs.scmjs.dev/chk/the-container/). Native packet and addon offset checks use the pinned [BWAPI command implementation](https://github.com/bwapi/bwapi/blob/d727fed68558c506163048ea889131d8cbb33915/bwapi/BWAPI/Source/BWAPI/GameCommands.cpp).

Reproduction tools: `make_command_fixture.py`, `live_command_check.py` and `report_m15.py`. Only run mutation checks in the fresh private per-race fixtures. `acceptance.json` explicitly retains false overall/buttonset/grounding/cliff gates.

## Building grounding and cliffs

Landed buildings now use the highest terrain sample over their native placement footprint, including interior samples. Airborne buildings retain center elevation behavior. This prevents higher corners cutting through the base, but can leave a gap on the downhill side; it does **not** establish complete footprint contact on slopes.

The grounding tool now supports explicit terrain inputs and actual offscreen stereo Metal rendering with eye readback. Flat and sloped runs each capture 94 building-flagged DAT entries at four angles: 752 PNGs total. Of these entries, 77 have native image chains; 17 unused/empty entries do not. All frames are nonempty, but terrain pixels alone can satisfy that check. These are capture/visibility checks, not a passing contact classifier or headset presentation proof.

| Gate | Result | Private evidence |
|---|---|---|
| Flat four-angle captures | Captured; full contact unaccepted | `grounding-flat-readback/grounding.json` |
| Slope four-angle captures | Captured; downhill gap unresolved | `grounding-slope-readback/grounding.json` |
| Cliff stretch | Failed; distorted/folded terrain remains | `cliff-rendered/106-0.png` through `106-3.png`, `cliff-selection.json` |
| Complete building footprint acceptance | No | Both grounding reports retain `completeGroundingVerified: false` |

The initial blank readback runs are retained separately. Corrected captures explicitly copy the rendered left eye before reading pixels. No cliff fix is claimed.

## Validation and 125-unit stress

Python: **282 passed plus 60 subtests**. Rust: **16 passed**. Final desktop/XR/fallback runtime checks: **175 / 180 / 180 passed**. Logs: `tests-python-final.log`, `tests-rust-final.log`, `runtime-*-final.log`; exact runtime reports: `runtime-desktop.json`, `runtime-xr.json`, `runtime-fallback.json`. Counts include the active terrain's conditional cases.

The unchanged M13 population fixture ran through the existing M14 measurement driver, redirected to `population/`, with 30 seconds warmup and 75.16 seconds measured. All 15 native samples retained **125 owned units**, including commanded Marine movement. There were 6,765 submitted-frame CPU/pacing samples and 6,764 interval/GPU samples; mean cadence was 11.1111 ms (about 90 fps).

| Metric | Result |
|---|---:|
| Frame cadence p95 / p99 | 11.95 / 12.25 ms |
| Stereo CPU encode p95 | 0.64 ms |
| Stereo GPU p95 / max | 4.32 / 30.02 ms |
| After-wait work p95 / max | 0.89 / 6.28 ms |
| Cadence estimated missed slots | 0 |
| After-wait overruns | 0 |
| GPU samples above 11.111 ms | 1 |

The GPU duration diagnostic estimated two budget slots for its one outlier; the frame-interval diagnostic observed no missed slots. These are distinct metrics. The run preserves the app pacing gate, but the GPU outlier is retained as a limitation rather than classified as harmless. Compared with the M14 population reference, after-wait p95 is higher (0.89 vs 0.67 ms), still below budget; cadence p95 is lower (11.95 vs 12.68 ms). No pacing tuning was performed, and no physical headset presentation claim follows from simulator timings.

Evidence: `population/live-final-metrics.json`, `population/live-state-samples.json`, `population/stress-orders.json`, `population/live-launch.json`. `final-binaries.json` records the measured builds. The dedicated viewer exited after measurement.

## Remaining gates

- Exact original command-button parity is not implemented. Candidate relationships are curated; native prerequisite evaluation does not export the original complete buttonset. Specialized gameplay such as Archon merges, ammunition production, Nydus linkage, nuclear targeting and infestation still needs completion/verification. Individual-passenger unload and exhaustive multi-selection semantics remain open.
- Every unavailable/basic command state, higher upgrade level, hero variant and target legality has not been exhaustively compared with the original UI. The fail-closed requirement interpreter can hide a valid command for an unknown expression.
- XR controller end-to-end checks and real Quest comfort/interaction acceptance remain open.
- Full sloped footprint contact and cliff stretching remain failed visual gates; nonempty images are not substitutes for rendered contact acceptance.

No game assets or build target directory were committed. `asset-upgrades/`, shared OXRSys and unrelated games were not modified or restarted. Pacing code is unchanged.

## Per-command native evidence

Command identifiers retain native numeric IDs. Research/upgrade titles map through the tables in `CommandCard.swift`; morph pairs are source and target unit IDs. The table includes setup observations explicitly named `start`, `position` or `setup`.

| Race | Command | Verified | Evidence |
|---|---|---|---|
| protoss | cancel-build | yes | `runtime/3d-proof/m15/protoss/gameplay-v4/cancel-build.json` |
| protoss | cancel-build-start | yes | `runtime/3d-proof/m15/protoss/gameplay-v4/cancel-build-start.json` |
| protoss | cancel-research | yes | `runtime/3d-proof/m15/protoss/research-v4/cancel-research.json` |
| protoss | cancel-research-start | yes | `runtime/3d-proof/m15/protoss/research-v4/cancel-research-start.json` |
| protoss | cancel-train | yes | `runtime/3d-proof/m15/protoss/gameplay-v4/cancel-train.json` |
| protoss | cancel-train-start | yes | `runtime/3d-proof/m15/protoss/gameplay-v4/cancel-train-start.json` |
| protoss | cancel-upgrade | yes | `runtime/3d-proof/m15/protoss/research-v4/cancel-upgrade.json` |
| protoss | cancel-upgrade-start | yes | `runtime/3d-proof/m15/protoss/research-v4/cancel-upgrade-start.json` |
| protoss | feedback-scout | yes | `runtime/3d-proof/m15/protoss/supplemental/feedback-scout.json` |
| protoss | load | yes | `runtime/3d-proof/m15/protoss/gameplay-v4/load.json` |
| protoss | rally-ground | yes | `runtime/3d-proof/m15/protoss/rally/rally-ground.json` |
| protoss | rally-unit | yes | `runtime/3d-proof/m15/protoss/rally/rally-unit.json` |
| protoss | research-19 | yes | `runtime/3d-proof/m15/protoss/research-v4/research-19.json` |
| protoss | research-20 | yes | `runtime/3d-proof/m15/protoss/research-v4/research-20.json` |
| protoss | research-21 | yes | `runtime/3d-proof/m15/protoss/research-v4/research-21.json` |
| protoss | research-22 | yes | `runtime/3d-proof/m15/protoss/research-v4/research-22.json` |
| protoss | research-25 | yes | `runtime/3d-proof/m15/protoss/research-v4/research-25.json` |
| protoss | research-27 | yes | `runtime/3d-proof/m15/protoss/research-v4/research-27.json` |
| protoss | research-31 | yes | `runtime/3d-proof/m15/protoss/research-v4/research-31.json` |
| protoss | spell-feedback | yes | `runtime/3d-proof/m15/protoss/supplemental/spell-feedback.json` |
| protoss | spell-hallucination | yes | `runtime/3d-proof/m15/protoss/gameplay-v4/spell-hallucination.json` |
| protoss | spell-maelstrom | yes | `runtime/3d-proof/m15/protoss/gameplay-v4/spell-maelstrom.json` |
| protoss | spell-mind_control | yes | `runtime/3d-proof/m15/protoss/gameplay-v4/spell-mind_control.json` |
| protoss | spell-recall | yes | `runtime/3d-proof/m15/protoss/gameplay-v4/spell-recall.json` |
| protoss | spell-stasis | yes | `runtime/3d-proof/m15/protoss/gameplay-v4/spell-stasis.json` |
| protoss | spell-storm | yes | `runtime/3d-proof/m15/protoss/gameplay-v4/spell-storm.json` |
| protoss | spell-web | yes | `runtime/3d-proof/m15/protoss/gameplay-v4/spell-web.json` |
| protoss | unload | yes | `runtime/3d-proof/m15/protoss/gameplay-v4/unload.json` |
| protoss | upgrade-13 | yes | `runtime/3d-proof/m15/protoss/research-v4/upgrade-13.json` |
| protoss | upgrade-14 | yes | `runtime/3d-proof/m15/protoss/research-v4/upgrade-14.json` |
| protoss | upgrade-15 | yes | `runtime/3d-proof/m15/protoss/research-v4/upgrade-15.json` |
| protoss | upgrade-33 | yes | `runtime/3d-proof/m15/protoss/research-v4/upgrade-33.json` |
| protoss | upgrade-34 | yes | `runtime/3d-proof/m15/protoss/research-v4/upgrade-34.json` |
| protoss | upgrade-35 | yes | `runtime/3d-proof/m15/protoss/research-v4/upgrade-35.json` |
| protoss | upgrade-36 | yes | `runtime/3d-proof/m15/protoss/research-v4/upgrade-36.json` |
| protoss | upgrade-37 | yes | `runtime/3d-proof/m15/protoss/research-v4/upgrade-37.json` |
| protoss | upgrade-38 | yes | `runtime/3d-proof/m15/protoss/research-v4/upgrade-38.json` |
| protoss | upgrade-39 | yes | `runtime/3d-proof/m15/protoss/research-v4/upgrade-39.json` |
| protoss | upgrade-40 | yes | `runtime/3d-proof/m15/protoss/research-v4/upgrade-40.json` |
| protoss | upgrade-41 | yes | `runtime/3d-proof/m15/protoss/research-v4/upgrade-41.json` |
| protoss | upgrade-42 | yes | `runtime/3d-proof/m15/protoss/research-v4/upgrade-42.json` |
| protoss | upgrade-43 | yes | `runtime/3d-proof/m15/protoss/research-v4/upgrade-43.json` |
| protoss | upgrade-44 | yes | `runtime/3d-proof/m15/protoss/research-v4/upgrade-44.json` |
| protoss | upgrade-47 | yes | `runtime/3d-proof/m15/protoss/research-v4/upgrade-47.json` |
| protoss | upgrade-49 | yes | `runtime/3d-proof/m15/protoss/research-v4/upgrade-49.json` |
| protoss | upgrade-5 | yes | `runtime/3d-proof/m15/protoss/research-v4/upgrade-5.json` |
| protoss | upgrade-6 | yes | `runtime/3d-proof/m15/protoss/research-v4/upgrade-6.json` |
| terran | addon-107 | yes | `runtime/3d-proof/m15/terran/gameplay-v4/addon-107.json` |
| terran | addon-108 | yes | `runtime/3d-proof/m15/terran/gameplay-v4/addon-108.json` |
| terran | addon-115 | yes | `runtime/3d-proof/m15/terran/gameplay-v4/addon-115.json` |
| terran | addon-117 | yes | `runtime/3d-proof/m15/terran/gameplay-v4/addon-117.json` |
| terran | addon-118 | yes | `runtime/3d-proof/m15/terran/gameplay-v4/addon-118.json` |
| terran | addon-120 | yes | `runtime/3d-proof/m15/terran/gameplay-v4/addon-120.json` |
| terran | cancel-addon | yes | `runtime/3d-proof/m15/terran/additional/cancel-addon.json` |
| terran | cancel-addon-start | yes | `runtime/3d-proof/m15/terran/additional/cancel-addon-start.json` |
| terran | cancel-build | yes | `runtime/3d-proof/m15/terran/gameplay-v4/cancel-build.json` |
| terran | cancel-build-start | yes | `runtime/3d-proof/m15/terran/gameplay-v4/cancel-build-start.json` |
| terran | cancel-research | yes | `runtime/3d-proof/m15/terran/research-v4/cancel-research.json` |
| terran | cancel-research-start | yes | `runtime/3d-proof/m15/terran/research-v4/cancel-research-start.json` |
| terran | cancel-train | yes | `runtime/3d-proof/m15/terran/gameplay-v4/cancel-train.json` |
| terran | cancel-train-start | yes | `runtime/3d-proof/m15/terran/gameplay-v4/cancel-train-start.json` |
| terran | cancel-upgrade | yes | `runtime/3d-proof/m15/terran/research-v4/cancel-upgrade.json` |
| terran | cancel-upgrade-start | yes | `runtime/3d-proof/m15/terran/research-v4/cancel-upgrade-start.json` |
| terran | cloak | yes | `runtime/3d-proof/m15/terran/gameplay-v4/cloak.json` |
| terran | decloak | yes | `runtime/3d-proof/m15/terran/gameplay-v4/decloak.json` |
| terran | heal-position | yes | `runtime/3d-proof/m15/terran/supplemental/heal-position.json` |
| terran | land | yes | `runtime/3d-proof/m15/terran/gameplay-v4/land.json` |
| terran | lift | yes | `runtime/3d-proof/m15/terran/gameplay-v4/lift.json` |
| terran | load | yes | `runtime/3d-proof/m15/terran/gameplay-v4/load.json` |
| terran | position-34 | yes | `runtime/3d-proof/m15/terran/supplemental/position-34.json` |
| terran | position-9 | yes | `runtime/3d-proof/m15/terran/supplemental/position-9.json` |
| terran | rally-ground | yes | `runtime/3d-proof/m15/terran/rally/rally-ground.json` |
| terran | rally-unit | yes | `runtime/3d-proof/m15/terran/rally/rally-unit.json` |
| terran | research-0 | yes | `runtime/3d-proof/m15/terran/research-v4/research-0.json` |
| terran | research-1 | yes | `runtime/3d-proof/m15/terran/research-v4/research-1.json` |
| terran | research-10 | yes | `runtime/3d-proof/m15/terran/research-v4/research-10.json` |
| terran | research-2 | yes | `runtime/3d-proof/m15/terran/research-v4/research-2.json` |
| terran | research-24 | yes | `runtime/3d-proof/m15/terran/research-v4/research-24.json` |
| terran | research-3 | yes | `runtime/3d-proof/m15/terran/research-v4/research-3.json` |
| terran | research-30 | yes | `runtime/3d-proof/m15/terran/research-v4/research-30.json` |
| terran | research-5 | yes | `runtime/3d-proof/m15/terran/research-v4/research-5.json` |
| terran | research-7 | yes | `runtime/3d-proof/m15/terran/research-v4/research-7.json` |
| terran | research-8 | yes | `runtime/3d-proof/m15/terran/research-v4/research-8.json` |
| terran | research-9 | yes | `runtime/3d-proof/m15/terran/research-v4/research-9.json` |
| terran | restore-irradiate-setup | yes | `runtime/3d-proof/m15/terran/supplemental/restore-irradiate-setup.json` |
| terran | siege | yes | `runtime/3d-proof/m15/terran/gameplay-v4/siege.json` |
| terran | spell-emp | yes | `runtime/3d-proof/m15/terran/gameplay-v4/spell-emp.json` |
| terran | spell-flare | yes | `runtime/3d-proof/m15/terran/additional/spell-flare.json` |
| terran | spell-heal | yes | `runtime/3d-proof/m15/terran/additional/spell-heal.json` |
| terran | spell-irradiate | yes | `runtime/3d-proof/m15/terran/gameplay-v4/spell-irradiate.json` |
| terran | spell-lockdown | yes | `runtime/3d-proof/m15/terran/gameplay-v4/spell-lockdown.json` |
| terran | spell-matrix | yes | `runtime/3d-proof/m15/terran/gameplay-v4/spell-matrix.json` |
| terran | spell-mine | yes | `runtime/3d-proof/m15/terran/gameplay-v4/spell-mine.json` |
| terran | spell-restoration | yes | `runtime/3d-proof/m15/terran/supplemental/spell-restoration.json` |
| terran | spell-scan | yes | `runtime/3d-proof/m15/terran/gameplay-v4/spell-scan.json` |
| terran | spell-yamato | yes | `runtime/3d-proof/m15/terran/gameplay-v4/spell-yamato.json` |
| terran | stim | yes | `runtime/3d-proof/m15/terran/gameplay-v4/stim.json` |
| terran | unload | yes | `runtime/3d-proof/m15/terran/gameplay-v4/unload.json` |
| terran | unsiege | yes | `runtime/3d-proof/m15/terran/gameplay-v4/unsiege.json` |
| terran | upgrade-0 | yes | `runtime/3d-proof/m15/terran/research-v4/upgrade-0.json` |
| terran | upgrade-1 | yes | `runtime/3d-proof/m15/terran/research-v4/upgrade-1.json` |
| terran | upgrade-16 | yes | `runtime/3d-proof/m15/terran/research-v4/upgrade-16.json` |
| terran | upgrade-17 | yes | `runtime/3d-proof/m15/terran/research-v4/upgrade-17.json` |
| terran | upgrade-19 | yes | `runtime/3d-proof/m15/terran/research-v4/upgrade-19.json` |
| terran | upgrade-2 | yes | `runtime/3d-proof/m15/terran/research-v4/upgrade-2.json` |
| terran | upgrade-20 | yes | `runtime/3d-proof/m15/terran/research-v4/upgrade-20.json` |
| terran | upgrade-21 | yes | `runtime/3d-proof/m15/terran/research-v4/upgrade-21.json` |
| terran | upgrade-22 | yes | `runtime/3d-proof/m15/terran/research-v4/upgrade-22.json` |
| terran | upgrade-23 | yes | `runtime/3d-proof/m15/terran/research-v4/upgrade-23.json` |
| terran | upgrade-51 | yes | `runtime/3d-proof/m15/terran/research-v4/upgrade-51.json` |
| terran | upgrade-54 | yes | `runtime/3d-proof/m15/terran/research-v4/upgrade-54.json` |
| terran | upgrade-7 | yes | `runtime/3d-proof/m15/terran/research-v4/upgrade-7.json` |
| terran | upgrade-8 | yes | `runtime/3d-proof/m15/terran/research-v4/upgrade-8.json` |
| terran | upgrade-9 | yes | `runtime/3d-proof/m15/terran/research-v4/upgrade-9.json` |
| zerg | burrow | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/burrow.json` |
| zerg | cancel-build | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/cancel-build.json` |
| zerg | cancel-build-start | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/cancel-build-start.json` |
| zerg | cancel-building-morph | yes | `runtime/3d-proof/m15/zerg/additional/cancel-building-morph.json` |
| zerg | cancel-building-morph-start | yes | `runtime/3d-proof/m15/zerg/additional/cancel-building-morph-start.json` |
| zerg | cancel-morph | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/cancel-morph.json` |
| zerg | cancel-morph-start | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/cancel-morph-start.json` |
| zerg | cancel-research | yes | `runtime/3d-proof/m15/zerg/research-v4/cancel-research.json` |
| zerg | cancel-research-start | yes | `runtime/3d-proof/m15/zerg/research-v4/cancel-research-start.json` |
| zerg | cancel-upgrade | yes | `runtime/3d-proof/m15/zerg/research-v4/cancel-upgrade.json` |
| zerg | cancel-upgrade-start | yes | `runtime/3d-proof/m15/zerg/research-v4/cancel-upgrade-start.json` |
| zerg | load | yes | `runtime/3d-proof/m15/zerg/supplemental/load.json` |
| zerg | morph-131-132 | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/morph-131-132.json` |
| zerg | morph-132-133 | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/morph-132-133.json` |
| zerg | morph-141-137 | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/morph-141-137.json` |
| zerg | morph-143-144 | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/morph-143-144.json` |
| zerg | morph-143-146 | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/morph-143-146.json` |
| zerg | morph-35-37 | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/morph-35-37.json` |
| zerg | morph-35-38 | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/morph-35-38.json` |
| zerg | morph-35-39 | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/morph-35-39.json` |
| zerg | morph-35-41 | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/morph-35-41.json` |
| zerg | morph-35-42 | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/morph-35-42.json` |
| zerg | morph-35-43 | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/morph-35-43.json` |
| zerg | morph-35-45 | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/morph-35-45.json` |
| zerg | morph-35-46 | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/morph-35-46.json` |
| zerg | morph-35-47 | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/morph-35-47.json` |
| zerg | morph-38-103 | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/morph-38-103.json` |
| zerg | morph-43-44 | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/morph-43-44.json` |
| zerg | morph-43-62 | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/morph-43-62.json` |
| zerg | rally-ground | yes | `runtime/3d-proof/m15/zerg/rally/rally-ground.json` |
| zerg | rally-unit | yes | `runtime/3d-proof/m15/zerg/rally/rally-unit.json` |
| zerg | research-11 | yes | `runtime/3d-proof/m15/zerg/research-v4/research-11.json` |
| zerg | research-13 | yes | `runtime/3d-proof/m15/zerg/research-v4/research-13.json` |
| zerg | research-15 | yes | `runtime/3d-proof/m15/zerg/research-v4/research-15.json` |
| zerg | research-16 | yes | `runtime/3d-proof/m15/zerg/research-v4/research-16.json` |
| zerg | research-17 | yes | `runtime/3d-proof/m15/zerg/research-v4/research-17.json` |
| zerg | research-32 | yes | `runtime/3d-proof/m15/zerg/research-v4/research-32.json` |
| zerg | spell-broodling | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/spell-broodling.json` |
| zerg | spell-consume | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/spell-consume.json` |
| zerg | spell-ensnare | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/spell-ensnare.json` |
| zerg | spell-parasite | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/spell-parasite.json` |
| zerg | spell-plague | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/spell-plague.json` |
| zerg | spell-swarm | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/spell-swarm.json` |
| zerg | unburrow | yes | `runtime/3d-proof/m15/zerg/gameplay-v4/unburrow.json` |
| zerg | unload | yes | `runtime/3d-proof/m15/zerg/supplemental/unload.json` |
| zerg | upgrade-10 | yes | `runtime/3d-proof/m15/zerg/research-v4/upgrade-10.json` |
| zerg | upgrade-11 | yes | `runtime/3d-proof/m15/zerg/research-v4/upgrade-11.json` |
| zerg | upgrade-12 | yes | `runtime/3d-proof/m15/zerg/research-v4/upgrade-12.json` |
| zerg | upgrade-24 | yes | `runtime/3d-proof/m15/zerg/research-v4/upgrade-24.json` |
| zerg | upgrade-25 | yes | `runtime/3d-proof/m15/zerg/research-v4/upgrade-25.json` |
| zerg | upgrade-26 | yes | `runtime/3d-proof/m15/zerg/research-v4/upgrade-26.json` |
| zerg | upgrade-27 | yes | `runtime/3d-proof/m15/zerg/research-v4/upgrade-27.json` |
| zerg | upgrade-28 | yes | `runtime/3d-proof/m15/zerg/research-v4/upgrade-28.json` |
| zerg | upgrade-29 | yes | `runtime/3d-proof/m15/zerg/research-v4/upgrade-29.json` |
| zerg | upgrade-3 | yes | `runtime/3d-proof/m15/zerg/research-v4/upgrade-3.json` |
| zerg | upgrade-30 | yes | `runtime/3d-proof/m15/zerg/research-v4/upgrade-30.json` |
| zerg | upgrade-31 | yes | `runtime/3d-proof/m15/zerg/research-v4/upgrade-31.json` |
| zerg | upgrade-32 | yes | `runtime/3d-proof/m15/zerg/research-v4/upgrade-32.json` |
| zerg | upgrade-4 | yes | `runtime/3d-proof/m15/zerg/research-v4/upgrade-4.json` |
| zerg | upgrade-52 | yes | `runtime/3d-proof/m15/zerg/research-v4/upgrade-52.json` |
| zerg | upgrade-53 | yes | `runtime/3d-proof/m15/zerg/research-v4/upgrade-53.json` |
