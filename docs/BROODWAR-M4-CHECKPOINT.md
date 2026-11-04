# Brood War M4 live acceptance

Work in progress. Evidence is private under
`~/.local/share/soju/broodwar-native/3d-proof/m4/`.

## Map startup

The blocked client was at frame 0 behind the StarCraft Tips modal, not in
the main menu. `startup.png` shows the original failure. The legacy registry
`tip` setting did not suppress it. SCR's isolated
`documents/StarCraft/CSettings.json` setting `"tip": 0` does.

Map launch now preserves other JSON preferences, atomically disables startup
tips, and requires two advancing native snapshots of the requested map before
printing `running: true`. It times out after 120 seconds rather than treating
the bridge listener as gameplay success. Menu launches keep their existing
behavior. No UI attachment is used.

Two consecutive dedicated-bottle restarts reached native frames 0 → 8:
`startup-restart-1.log` and `startup-restart-2.log`.
`startup-tip-disabled.png` and `start-state.json` show the running fixture.

## Extended commands

`make_acceptance_fixture.py` creates a private owned-map derivative with Terran,
Zerg and Protoss producers, transport and mode users. The one-shot resource
trigger and technology availability use the native
[CHK tables](https://docs.scmjs.dev/chk/). No assets are generated in the repository.

`live_extended_check.py` observed all seven named abilities, transport load and
unload, train queue/cancel/completion, research/upgrade start and full refund,
and larva morph/cancellation. `commands/results.json` records native frames and
each corresponding JSON captures the complete native state. The first test
incorrectly expected egg cancellation to preserve the larva handle; the corrected
check observes the disappearing egg and resource refund. Failed evidence is
retained as `commands/cancel_morph-failed.json`.

The original `verify.py` passed gather, SCV training, depot construction and
follow in `live-basic.log`. Desktop and OpenXR viewers ran concurrently;
`commands/{desktop,xr}-renderer.json` and `desktop-xr-acceptance.png` record them.
The simulator was explicitly reconnected to the Brood War stream and received
72 fps. Physical headset acceptance is not claimed.

The larger fixture exposes missing models for types 114, 116, 123 and 135.
Addon, building-construction cancellation, rally and repair still need dedicated
extended acceptance checks. Targeted spells and passenger-specific unload remain
unimplemented.

Remaining: presentation additions, wrist UX,
OpenXR controller acceptance, and the two-minute demo.
