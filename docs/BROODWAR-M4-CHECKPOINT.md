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

Remaining: extended-command acceptance, presentation additions, wrist UX,
OpenXR controller acceptance, and the two-minute demo.
