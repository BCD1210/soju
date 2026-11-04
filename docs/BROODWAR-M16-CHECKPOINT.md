# Brood War M16 checkpoint

M16 is **partial; the M15 remaining gates are not closed**. Work starts at `b5500ac` on `feat/broodwar-3d`. All changes are local commits; nothing was pushed. `runtime/` means `~/.local/share/soju/broodwar-native/`. Evidence is under `runtime/3d-proof/m16/`.

## Original button records and parity

The authenticated `buttonsets` request reads all 250 original button sets from the loaded game process, including the 228 unit sets and 22 additional sets. It exports original positions, icons, condition/action addresses, parameters, string IDs and linked-set values. The reader locates the table in the loaded executable's `.data` section, rejects ambiguous matches and copies memory through `ReadProcessMemory`; it does not dereference unvalidated foreign addresses in Rust. The locator/layout reference is [Samase's analysis](https://github.com/neivv/samase_scarf/blob/f1681dbac8c7d1ac184dc84918ab7dd5771061ed/src/firegraft.rs), with the [64-bit layouts](https://github.com/neivv/samase_scarf/blob/f1681dbac8c7d1ac184dc84918ab7dd5771061ed/src/struct_layouts.rs).

The action UI still uses the existing curated relationships. **The new reader does not yet drive it.** Original condition callbacks, client-selection context, submenu state and localized displayed strings have not been integrated into the UI. The readback explicitly says `evaluated: false`.

`audit_native_buttons.py` compares actual commander candidate slots/hotkeys with the original candidate records and hotkey bytes from the owned installation's `rez/stat_txt.tbl`. This is a structural rejection check, not an exact enabled-state comparison. It cannot certify passing parity. The Marine alone has five differences: Stop/Attack and Patrol/Hold are swapped, and Stim is in commander slot 6 versus original slot 7. Higher upgrade levels and hero availability remain unverified. Exact state mismatch count is **not measured**, not zero.

Evidence: `original-buttonsets-final.json`, `original-buttonsets-250.json`, `stat_txt.tbl`, `current-card-audit.json`, `button-parity.json`. The earlier diagnostic capture also contains callback bytes; all executable/resource data stays outside git.

The final fixture audit covers **76 types**, with **347 candidate slot/hotkey differences**. Missing buttons, visibility and disabled states require further comparison; these 347 differences are not a complete mismatch count. The complete per-type structural table follows. A zero in its last column does not establish parity.

| Unit type ID | Original candidates | Commander candidates | Candidate layout differences |
|---|---:|---:|---:|
| 0 | 6 | 6 | 5 |
| 1 | 9 | 8 | 7 |
| 2 | 6 | 6 | 5 |
| 5 | 7 | 6 | 5 |
| 7 | 9 | 19 | 17 |
| 8 | 7 | 6 | 5 |
| 9 | 8 | 8 | 7 |
| 11 | 7 | 7 | 6 |
| 12 | 6 | 6 | 5 |
| 14 | 0 | 5 | 5 |
| 34 | 7 | 8 | 7 |
| 35 | 9 | 9 | 6 |
| 37 | 7 | 6 | 5 |
| 38 | 8 | 7 | 6 |
| 41 | 9 | 18 | 17 |
| 42 | 7 | 7 | 6 |
| 43 | 7 | 7 | 6 |
| 45 | 9 | 9 | 8 |
| 46 | 9 | 9 | 8 |
| 60 | 6 | 6 | 5 |
| 61 | 7 | 5 | 4 |
| 63 | 7 | 8 | 7 |
| 64 | 7 | 21 | 20 |
| 65 | 5 | 5 | 4 |
| 66 | 5 | 5 | 4 |
| 67 | 8 | 7 | 6 |
| 68 | 5 | 5 | 4 |
| 69 | 7 | 7 | 6 |
| 71 | 7 | 7 | 6 |
| 72 | 7 | 5 | 4 |
| 73 | 0 | 0 | 0 |
| 83 | 7 | 5 | 4 |
| 85 | 0 | 0 | 0 |
| 106 | 10 | 5 | 5 |
| 107 | 1 | 1 | 1 |
| 108 | 2 | 0 | 0 |
| 109 | 0 | 0 | 0 |
| 111 | 11 | 6 | 6 |
| 112 | 7 | 5 | 5 |
| 113 | 12 | 6 | 6 |
| 114 | 13 | 8 | 7 |
| 115 | 4 | 2 | 2 |
| 116 | 12 | 6 | 6 |
| 117 | 6 | 4 | 4 |
| 118 | 4 | 2 | 2 |
| 120 | 6 | 4 | 4 |
| 122 | 7 | 3 | 3 |
| 123 | 5 | 4 | 4 |
| 125 | 2 | 2 | 2 |
| 130 | 7 | 2 | 2 |
| 131 | 5 | 4 | 4 |
| 132 | 9 | 7 | 7 |
| 133 | 8 | 6 | 5 |
| 134 | 1 | 0 | 0 |
| 135 | 5 | 3 | 3 |
| 136 | 5 | 3 | 3 |
| 138 | 5 | 3 | 3 |
| 139 | 4 | 3 | 3 |
| 140 | 3 | 2 | 2 |
| 141 | 4 | 3 | 3 |
| 142 | 3 | 2 | 2 |
| 143 | 2 | 2 | 2 |
| 154 | 3 | 2 | 2 |
| 155 | 5 | 4 | 4 |
| 156 | 0 | 0 | 0 |
| 159 | 3 | 2 | 2 |
| 160 | 6 | 5 | 5 |
| 163 | 2 | 1 | 1 |
| 164 | 4 | 3 | 3 |
| 165 | 8 | 6 | 6 |
| 166 | 4 | 3 | 3 |
| 167 | 6 | 5 | 4 |
| 169 | 7 | 5 | 5 |
| 170 | 5 | 3 | 3 |
| 171 | 4 | 3 | 3 |
| 172 | 1 | 0 | 0 |

## Specialized native commands

New handlers submit one original selection packet for a merge group, then the original merge, ammunition, Nydus-exit or passenger-unload packet. Snapshots now expose current button-set ID, ammunition count, Nydus link and completed silo missile. Nuclear targeting uses order `0x7e`, confirmed by the loaded Ghost button callback; the first attempt using `0x80` failed and is retained. Infestation uses the original targeted handler.

| Command | Native result | Evidence |
|---|---|---|
| Archon merge | Pass: source pair consumed; Archon produced | `protoss/merge_archon.json` |
| Dark Archon merge | Pass: source pair consumed; Dark Archon produced | `protoss/merge_dark_archon.json` |
| Interceptor production | Pass: Carrier ammunition increased | `protoss/ammunition-72.json` |
| Scarab production | Pass: Reaver ammunition increased | `protoss/ammunition-83.json` |
| Nydus exit | Pass: completed entrance/exit have reciprocal native links | `zerg/nydus-link.json` |
| Nuclear Missile build | Pass: completed missile attached to silo | `terran-v2/nuclear-missile-build.json` |
| Nuclear targeting | Pass: silo released missile; native missile launch order observed | `terran-v2/nuclear-target.json` |
| Infest Command Center | Pass: same target handle became owned type 130 | `infest/infest-command-center.json` |
| Individual passenger unload | Pass after stopping: Probe unloaded; Zealot stayed aboard; cargo count 1 | `passengers-v2/unload-individual.json` |
| Mixed 12-unit move | Pass: every selected unit changed position | `infest/mixed-12-move.json` |
| Mixed 12-unit stop | Pass: every selected unit reached zero speed | `infest/mixed-12-stop.json` |

These are bridge-to-original-handler native transitions, not controller or complete action-card acceptance. Merge, ammunition, Nydus and passenger controls still need integration with the original-driven card. Mixed-group shared-card filtering, odd-sized merge groups, hero semantics and exhaustive target legality are not certified. Nydus passage and nuclear blast damage were not separately asserted.

Failed attempts remain at `passengers/unload-individual.json` (transport still executing load) and `terran/nuclear-target.json` (wrong targeting order). The successful unload includes a separately recorded stop setup. Other setup/load/scout observations are retained in each directory's `results.json` and are not counted as distinct specialized commands.

The private fixtures preserve original unit durations and native costs, while retaining M15's shortened research/upgrade durations. Protoss adds two Dark Templars, a Carrier and a Reaver; Zerg adds a Nydus Canal. The infestation variant replaces one enemy tank with a 40%-HP Command Center. Maps, provenance, drivers and before/after snapshots remain in the M16 evidence directory.

## XR controller table

| Race | Selection → card → targeting | Result |
|---|---|---|
| Terran | Attempted live viewer; no controller chain completed | Head poses valid, but `aimValid=0` and `leftValid=0` |
| Zerg | Not completed | Same unresolved simulator-controller routing |
| Protoss | Not completed | Same unresolved simulator-controller routing |

The first short launch exited on a prepared-map mismatch. After preparing the correct terrain, the second launch advanced native frames and rendered XR frames but reported invalid controller poses. The computer-control surface requested by the dedicated simulator path showed a stale D2R window, so no key or mouse input was sent to it. Only the Brood War viewer was terminated. Both simulator processes and the other job were left running.

Evidence: `xr-terran.log`, `xr-terran-controller.json`, `xr-terran-metrics.json`. `xr-terran-input.json` is an old input snapshot copied for diagnosis; its timestamp predates this run and it is **not** acceptance evidence. No controller end-to-end pass or Quest acceptance is claimed.

## Rendered visual gates

| Gate | Fresh captures | Acceptance |
|---|---|---|
| Sloped building footprint | 94 building-flagged entries × 4 views = 376 Metal readbacks | Failed/unaccepted; no complete contact classifier |
| Representative slope views | Command Center, Hatchery, Nexus × 4 views | Full downhill contact not established |
| Cliff stretching | Command Center cliff scene × 4 views | Failed: stretched/folded faces and black gaps |

Evidence: `grounding-slope-all/grounding.json`, `grounding-slope-readback/`, `cliff/grounding.json`. The all-entry run has 77 entries with native image chains and 17 empty/unused entries. Nonempty images are not accepted as contact proof.

`cliff/106-1.png` clearly shows the failed cliff surfaces. `Terrain3D.worldPosition` applies `z + height / tan(32°)`; across steep descending samples its depth derivative can reverse, folding the reconstructed terrain. No cliff fix was made. Building placement still uses maximum sampled footprint height, and relief geometry anchors its lowest opaque vertex. This does not make the complete base conform to lower terrain. The renderer and assets were not changed to disguise these failures.

## Validation and pacing

The final population run measured 45.24 seconds after a 30-second warm-up, with 4,073 stereo frames. All nine native samples contained 125 owned units; the final rendered snapshot contained 137 total units. Ten group-move submissions each addressed 12 Marines. This measurement is shorter than the earlier 75-second run and is not a long-duration stability claim.

| Metric | p95 | p99 | Maximum |
|---|---:|---:|---:|
| Frame cadence | 11.87 ms | 12.40 ms | 13.87 ms |
| After-wait host work | 0.67 ms | 1.75 ms | 3.34 ms |
| Stereo GPU | 5.94 ms | 7.55 ms | 9.11 ms |
| Stereo CPU encode | 0.49 ms | 0.55 ms | 0.71 ms |

Estimated missed cadence slots: **0**. After-wait period overruns: **0**. GPU samples over 11.111 ms: **0**. Cadence samples over 11.111 ms: 2,054; these are not the missed-slot estimate. GPU p95 increased from M15's 4.32 ms; cadence p95 decreased from 11.95 ms. These timings are host/GPU measurements, not physical display-presentation telemetry.

Evidence: `population/live-final-metrics.json`, `population/live-state-samples.json`, `population/stress-orders.json`, `population/live.log`. The driver terminated only its own Brood War viewer. The shared simulator was still running afterward.

Final automated validation:

- Python: 282 passed and 60 subtests passed in 40.67 seconds (`tests-python-final.log`).
- Rust: 17 passed (`tests-rust-final.log`).
- Miri: 17 passed with strict provenance, symbolic alignment and preemption checks (`miri-specialized.log`); this covers executed Rust test paths, not the Windows process-memory FFI.
- A deliberately incorrect Dark Archon opcode failed the merge regression test; restoration passed (`merge-mutation.log`, `merge-restored.log`).
- Runtime checks: 176 desktop / 181 XR / 181 fallback checks passed (`runtime-desktop.json`, `runtime-xr.json`, `runtime-fallback.json`). These are automated renderer/runtime checks, not simulator-controller acceptance.

## Remaining work

- Drive the action UI from native evaluated button sets, including original positions, hotkeys, submenus and enabled states; compare every fixture type against the original UI with zero mismatches. The current structural audit rejects parity.
- Integrate specialized controls and original multi-selection/shared-command semantics. Complete higher-level upgrade and hero-state comparisons.
- Resolve controller input routing without restarting the shared runtime, then perform each race's selection/card/targeting chain through simulator input. Real Quest interaction remains unverified.
- Fix the cliff reconstruction and full sloped footprint contact, then add rendered acceptance criteria capable of rejecting missing/floating/clipped buildings and distorted terrain.

No game assets or `tools/broodwar_native/target/` are committed. `asset-upgrades/` and the shared OXRSys source/runtime were not modified. D2R was not stopped. Pacing code is unchanged.
