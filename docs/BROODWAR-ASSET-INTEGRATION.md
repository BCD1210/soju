# Brood War 3D asset integration

The existing native desktop/OpenXR viewer is ready to accept replacement GLBs
without engine edits. Blizzard's original offline client remains the simulation
authority. This covers model loading and the prepared commander scene; it is not
a claim that every Brood War unit, map, effect or game mode is complete.

## Install a Dots delivery

Keep the complete delivered folder (.blend originals, GLBs, textures, previews,
validation and provenance) as the source archive. The installer copies the active
GLBs to a separate release; it does not modify or delete the source delivery.

Run from the battlenet-macos repository on Mike's Mac:

```sh
python3 scripts/broodwar-native.py assets-check /absolute/path/to/delivery
python3 scripts/broodwar-native.py assets-install /absolute/path/to/delivery
python3 scripts/broodwar-native.py assets-rollback
```

Installation validates every asset, runs the actual native renderer's rig and
animation checks, and then atomically activates one catalog. A running viewer
notices it within roughly one second. Each existing unit gets a fresh private
rig, preserving native identity, position, action and selection. Newly spawned
units automatically use the active definition. Failed installation leaves the
active pack unchanged. Rollback restores the previous catalog; release files and
original art remain available.

For development, `unit-0.glb`, `unit-7.glb`, etc. can also be placed in
`~/.local/share/soju/broodwar-native/models-3d`, with optional matching JSON
sidecars. Use the installer for final delivery to avoid exposing partial writes.
An explicitly installed catalog takes precedence over loose unit-ID files.
L forces a definition reload. The viewer retains an existing valid model if a
replacement cannot load and reports the failed type.

## File and coordinate contract

- Standard GLB 2.0, triangulated geometry, one embedded buffer.
- Embed PNG/JPEG textures; no external texture or buffer URLs, DDS, Draco or
  required extension dependencies. Dense accessors are required by validation.
- Y is up, +Z is model forward. Moving native units use `yawDegrees: 180` by
  default to align the model with the game's north-facing direction.
- Use a ground/foot pivot. `footPivot: true` can correct an offset export.
- `targetHeight` normalizes the imported model's bounding height in scene units;
  one terrain tile is one scene unit. If omitted, `scale` multiplies authored size.
  Known baseline IDs inherit their placement defaults.
- Bone names must be unique within the model. Blender names such as
  `Arm.L.001` are supported. Each native unit owns separate nodes and skin bones.
- Animation clips must have distinct names. Use aliases below or declare exact
  names in the catalog. Check visual fidelity and feet in the delivered proof
  renders separately from structural validation.

A pack may use the existing filenames (`marine.glb`, `scv.glb`, etc.), numeric
`unit-ID.glb` filenames, or an explicit `catalog.json`. Numeric IDs are original
client type IDs; any ID in 0–65535 can be configured without recompiling. Adding
a model does not add unsupported game commands or new simulation behavior.

```json
{
  "schemaVersion": 1,
  "models": {
    "0": {
      "name": "Marine",
      "file": "marine.glb",
      "targetHeight": 1.748,
      "footPivot": true,
      "yawDegrees": 180,
      "faceHeading": true,
      "animations": {
        "idle": ["Idle", "Stand_full"],
        "walk": ["Walk", "Walk_full"],
        "attack": ["Fire", "Attack_full"]
      },
      "requiredAnimations": ["idle", "walk", "attack"]
    }
  }
}
```

Each model entry can also set `elevation` for its vertical offset and `flightHeight` (default 1.5) for airborne clearance. `teamMaterials` lists exact material names to tint with the native player color; other authored PBR materials remain intact. `turretNodes` lists exact yaw pivots controlled by native turret heading.
Buildings normally use `faceHeading: false` and an explicit yaw.
Runtime overrides inherit baseline values for the same ID. New IDs should
declare their intended size, pivot, heading and required actions explicitly.

| Semantic action | Common clip aliases | Native state |
| --- | --- | --- |
| idle | Idle, Stand_full | Stationary |
| walk | Walk, Run, Walk_full | Position changes |
| attack | Attack, Attack_full | Native attack image animation |
| work | Work, Mine, Work_full | Mining/construction order |
| carryIdle | carry_idle, Stand Gather_full | Stationary with resources |
| carryWalk | carry_walk, Walk Gather_full | Moving with resources |
| construction | construction, Build A Stand_full | Incomplete building |
| death | Death, Death_full | Confirmed native Die event; one shot, then release |
| fly / flyIdle | fly / hover | Native airborne flag and motion |
| burrow / burrowed / unburrow | matching names | Native burrow orders and state |
| siege / unsiege / morph | matching names | Native deployment/morph orders |
| lift / land | takeoff / landing | Native building flight transition |
| cast / stunned / repair / produce | matching names | Spell animation, disabling timers, repair and train state |

Marine defaults require idle/walk/attack. SCV defaults also require work,
carryIdle and carryWalk. A missing optional action falls back to idle (or walk
for carryWalk) and is reported in diagnostics. Required actions reject the pack.
Authored PBR material values are preserved.

## Replaceable combat effects

A weapon-ID catalog uses the same GLB schema in a separate `effects-3d` folder.
It never inherits unit placement or animation defaults. Installation and rollback
have independent release histories:

```sh
python3 scripts/broodwar-native.py assets-install /absolute/path/to/effects --domain effects
python3 scripts/broodwar-native.py assets-rollback --domain effects
```

Native projectile positions drive these models. A small procedural marker is the
fallback until weapon art is supplied; native fire/damage drive brief muzzle and
impact markers. These are temporary effect visuals, not final Dots artwork.

## Build, run and inspect

```sh
python3 tools/broodwar_native/build_scene.py
python3 tools/broodwar_native/build_scene.py --xr
python3 scripts/broodwar-native.py model-check
python3 scripts/broodwar-native.py scene
python3 scripts/broodwar-native.py scene --xr
python3 -m unittest discover -s tests -p test_broodwar_assets.py
```

Keep one XR viewer running at a time. Use the existing original-client play/menu
flow. The scene launcher prepares the loaded map automatically and waits while
the original client is paused or loading. Native Z:, C:, and Maps/ paths are
resolved locally. To bootstrap the pinned read-only asset readers on another Mac:

```sh
python3 tools/broodwar_native/build_asset_deps.py
```

Prepared terrain is cached by native path and map-file hash under
`terrain-releases/`; `scene-assets` selects one release atomically. A legacy
terrain directory is preserved, failed exports leave it active, and prepared
releases are reused without exporting again. Dots unit/effect releases are
independent of terrain preparation.

Keyboard/mouse: WASD walk, drag look, 1 workers, 2 army, 3 bases, right-click
resource/enemy/ground for gather/attack/move, Q attack-move, H hold, P patrol,
G mine, F follow, T train a Terran/Protoss worker, B basic structure
(Depot/Pylon/Hatchery), X stop, R recenter, L reload.
The native build endpoint accepts all three standard workers and same-race,
non-addon buildings. The train endpoint delegates ordinary building production
to the original game. Zerg morph, research, upgrades, spells and the complete
production UI remain unfinished; a queued acknowledgement is not success.
XR retains left-stick walking, right-stick turning, left-trigger selection,
right-grip action cycling and right-trigger confirmation.

For a delivered pack before activation:

```sh
~/.local/share/soju/broodwar-native/soju-bw-scene \
  --model-proof --model-dir /absolute/path/to/delivery \
  --catalog /absolute/path/to/delivery/catalog.json --model-types 0,7 \
  --proof-dir /absolute/path/to/proofs
```

Use `--model-types` to limit an incomplete delivery to the IDs it contains.
The proof command reports triangle counts, clip binding, actual bone movement,
private skeleton ownership and cross-instance isolation, plus four view PNGs.
An installation release contains the same renderer check report under `proof/`.

Runtime evidence is under `~/.local/share/soju/broodwar-native`:

- `3d-xr-diagnostics.json` / `3d-desktop-diagnostics.json`: current game frame,
  loaded files, actions, missing types and asset errors.
- `integration-tests/final-proof/asset-checks.json`: eight baseline model checks.
- `integration-tests/alias-proof/asset-checks.json`: unknown ID 65000, renamed
  animation aliases and Blender-style dotted bone names, without engine edits.
- `integration-tests/live-integration.json`: original-client movement, mining,
  new worker spawn, follow, building completion, live install and rollback.
- `models-3d/releases/*/validation.json`: source hashes and activated pack checks.

## Verification and remaining work

On 2026-11-04 UTC, both desktop ARM64 and XR x86_64 builds compiled. Twelve Python
validation/activation tests passed. Eight baseline model types passed the native
renderer checks, including Marine/SCV bone animation and isolated instances.
Marine/SCV live installation and rollback passed while the original game kept
running. Mining showed work/carryWalk, SCVs increased from three to four, the new
worker appeared automatically, Follow moved a Marine, and a new Depot at
(3696,1952) reached 500 HP/completed and appeared in 3D.

A first requested Depot position at tile (121,60) produced no building; the
alternate (114,60) completed. A queued command acknowledgement is not construction
success. The intermediate construction animation was not captured in that live
run; its clip is covered by the renderer check.

The existing eight model types are Marine, SCV, Command Center, Supply Depot,
three mineral variants and geyser. Their visual fidelity is below the user's
target; final Dots replacements have not been delivered. The current loaded map is prepared automatically from the installed original
CASC data when the scene is launched. Protocol 2 exports up to 4096 visible objects within a 2 MiB response
limit; an oversized/incomplete response is rejected rather than partially applied.
Preparation records the native map path and file/terrain hashes to distinguish
maps with identical dimensions. The viewer hands map changes back to the launcher,
which selects a matching cached release or builds it before reopening the scene.
Complete control coverage, all-unit art/effect fidelity, and physical
headset/controller acceptance remain additional work. Simulator frames are not
proof of real headset usability.
## Native combat implementation checkpoint — 2026-11-04 UTC

The bridge samples native HP, shields, firing cooldowns and visible projectile
objects. A bounded 1024-event journal uses sequence numbers and a game-session
identity. Native order hooks report confirmed deaths; units disappearing into fog,
transport or refineries are never inferred to have died. A viewer joining a game
does not replay historical effects. New-game identities reset all old nodes.

Death clips play once. The rig remains until its clip/fade finishes, then releases;
missing death art uses a short fade. Current baseline models have no death clips,
so live acceptance proves event-driven retirement/fade. A mapped existing clip
separately tests one-shot binding; final Dots death animations need visual review.

Evidence:

- `runtime-checks.json`: 24 renderer checks, including independent deaths, replay
  suppression, fog removal, transport, cloak, flight, native state mappings,
  projectile cleanup, one-shot clip binding and game-session reset.
- `integration-tests/live-combat.json`: original-engine private combat fixture;
  10 firing events, 10 damage events, one confirmed death, 74 projectile samples;
  XR received combat events and retired the dead model, with no missing types.
- 25 Python/Swift transport, map identity and asset activation tests; two Rust journal tests;
  both desktop and XR builds; eight baseline GLB model checks.
- `integration-tests/effect-install`: isolated non-gameplay effect pack validated
  through the actual renderer, installed and rolled back independently.

Reproduce native combat only in a fresh `Soju-Combat-Test-*` private map with
`python3 tools/broodwar_native/live_combat_check.py`. The test intentionally
issues attack-move and targeted attack orders to two fixture Marines. It refuses
other maps and saves its result under `integration-tests/live-combat.json`.

Automatic map preparation additionally passed on the live private combat map,
including an immediate cache hit. `integration-tests/live-map-preparation.json`
records the native path, prepared path and recent XR frame for the same map.

`integration-tests/live-map-switch.json` confirms a running scene switched from
`Soju-Combat-Test-v2.scm` back to `v1`, selected the cached terrain, recreated the
OpenXR session and rendered the advancing native frame with no missing types.
The complete native combat acceptance was rerun after this map transition and
passed again. `runtime-checks.json` also verifies material isolation, so one
unit's team tint cannot change another unit's materials.
