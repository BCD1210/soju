# Community compatibility reports

These are user reports, not additional maintainer-verified games. Results can
depend on the game build, renderer, Mac and macOS version. Opening a store client
does not establish that its games work.

| Game / store | Reported environment | Result and limits | Source |
| --- | --- | --- | --- |
| Diablo II: Resurrected / Battle.net | M5 Air (issue title), macOS 27, Soju 1.6.6 | Reporter reached gameplay at medium settings, 40–60 FPS. Resolution and session duration not supplied; edition name awaits clarification. | [#49](https://github.com/BCD1210/soju/issues/49) |
| Dark Souls III / Steam | M5 Pro, macOS 26.6.2, Soju 1.6.6 | DXMT stalls for about one minute on enemy kills and Continue. Reporter says per-game DXVK + MoltenVK removes the stalls, at about 30 FPS with a CPU limit. See the opt-in [renderer switch](STEAM-GAMES.md#per-game-renderer). | [#48](https://github.com/BCD1210/soju/issues/48) |
| Honkai: Star Rail / Epic | MacBook Pro M5, macOS 26.3, Soju 1.6.5 | White screen, cannot reach gameplay. The report's selected “Works in-game” field conflicts with its description; **not a working-game confirmation**. Exact failing process and logs are still needed. | [#47](https://github.com/BCD1210/soju/issues/47) |
| Diablo II: Resurrected / Battle.net | M2 Max, macOS 26.0.1 | No game window. This OS is below the documented D2R minimum of 26.4. Soju 1.6.6 adds the missing preflight/diagnostic check; the reporter's result after updating macOS is still pending. | [#44](https://github.com/BCD1210/soju/issues/44) |

For Honkai: Star Rail, a white screen alone does not identify a renderer,
launcher or anti-cheat fault. Please include `soju doctor epic`, the last relevant
launch log lines, and whether the white window belongs to Epic, HoYoPlay or the
game. Remove account details and personal paths. The general Wine limitation for
kernel anti-cheat still applies; this project does not supply anti-cheat bypasses.
