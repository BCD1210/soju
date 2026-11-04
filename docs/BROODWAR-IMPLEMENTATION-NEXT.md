# Brood War native 3D implementation checkpoint

Updated 2026-11-04 UTC. This is a continuation checkpoint, not a claim of complete Brood War/VR support.

Latest partial continuation: [M15 command and visual checkpoint](BROODWAR-M15-CHECKPOINT.md).
See that report for the delivery gate, desktop controls, new command endpoints,
evidence and the live-client blocker. The older verification below predates it.

## Working implementation

The original, unmodified Blizzard 1.23.10.13515 offline client drives simulation through the pinned Samase API 44 plugin. `tools/broodwar_native/src/world.rs` exports typed native unit state, cooldown/HP change events, visible bullets and confirmed Die-order events. Events have bounded history, sequence numbers and per-game identity. Fog/transport disappearance never implies death.

`Models3D.swift`, `ModelCatalog.swift` and `CombatEffects.swift` support private rigs, data-defined placement/action aliases, optional team materials/turret pivots, airborne/cloaked/hidden state, one-shot death retirement, native projectiles, atomic live unit/effect releases and independent rollback. Walk/run assets should animate in place; original game positions drive locomotion. Current baseline art has eight types and lacks death clips; it is below the requested visual quality. Dots' latest visible report says no asset is approved at final quality yet.

`map_assets.py` prepares the loaded SCM/SCX from the owned CASC installation. Terrain releases are cached and activated atomically. The launcher waits through a paused/loading client, and exit 78 from the viewer triggers preparation/reopening for a changed map. Z:, C: and Maps/ native paths are supported. `build_asset_deps.py` builds pinned StormLib/CascLib; no game assets are downloaded.

Native command endpoints: move, attack_move, targeted attack using each type's DAT attack order, patrol, hold, stop, follow, mineral/owned-refinery gather, all three standard worker build orders, ordinary building training. Original game handlers determine build/train acceptance. A queued ACK is not successful production.

## Verified evidence

Under `~/.local/share/soju/broodwar-native`:

- `runtime-checks.json`: 24 deterministic renderer checks, including independent team materials and one-shot death binding.
- `integration-tests/live-combat.json`: native attack/damage/death plus actual XR retirement; 10 fire, 10 damage, one death, 74 sampled bullets in the latest run.
- `integration-tests/live-map-preparation.json` and `live-map-switch.json`: native map identity, terrain activation and advancing XR frames across a v2-to-v1 same-dimension map transition.
- `3d-xr-diagnostics.json`: running viewer, current frame, loaded files, missing types, asset/effect errors, event counters and retired models.
- `3d-proof/asset-checks.json`: eight baseline model/rig checks.
- Separate effects install through the actual renderer and rollback passed for isolated type 65000.

Both desktop ARM64 and OpenXR x86_64 builds pass. 25 Python/Swift map/transport/activation tests and two Rust journal tests pass. The current game is the private `Soju-Combat-Test-v1.scm`; do not restart unrelated Wine bottles or projects.

## Work still required

1. Finish exact native command-card parity and controller acceptance. M15 verifies native outcomes for 17 Zerg morphs, six Terran addons, 24 research entries, 50 upgrades, 24 spells, transport, rally, modes and cancellation. The selected card now queries native prerequisites and costs, but candidate relationships remain curated; specialized commands and full native buttonset parity remain open. See the M15 per-command evidence table. Do not claim all-race gameplay is complete.
2. Import all final Dots source deliveries without discarding originals; map every unit/form/clip/effect and verify authored scale, pivot, turret and team-material conventions. Final assets have not been supplied/approved. Existing eight-type fixtures cannot establish all-unit visual coverage.
3. Validate other terrain layouts/tilesets, broader live state transitions and large armies. Protocol currently caps 4096 visible objects and 2 MiB total response; oversized data is rejected atomically. Fragmented transport has synthetic 2 MiB tests, not full-size live stress acceptance.
4. Finish authored combat/ability visuals; procedural projectile/muzzle/impact markers remain fallbacks. Native data exists, but visual fidelity and specialized effect timing need actual assets.
5. Real headset/controller comfort, tracking, interaction, performance and visual acceptance. OpenXR simulator frames do not prove physical headset usability.

## Commands

```sh
python3 scripts/broodwar-native.py build
python3 scripts/broodwar-native.py scene-build
python3 scripts/broodwar-native.py scene-build --xr
~/.local/share/soju/broodwar-native/soju-bw-scene --runtime-check
PYTHONPATH=tools/broodwar_native python3 -m unittest tools/broodwar_native/test_map_assets.py tools/broodwar_native/test_wire.py tests/test_broodwar_assets.py
python3 scripts/broodwar-native.py scene --xr
```

`live_combat_check.py` issues combat orders only in a fresh private combat fixture. See `BROODWAR-ASSET-INTEGRATION.md` for release installation, effect domains, exact schemas and limitations. Never print `session-token` or credential material while checking state.
