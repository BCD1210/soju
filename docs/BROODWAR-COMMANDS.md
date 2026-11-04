# Native commander command extensions

Commands run only in the dedicated offline client. Ownership, live handles,
sequence numbers and the original 12-unit selection limit still apply.
Production and mode changes select one native unit, then call the original
command handler. `queued: true` means submitted, not completed or accepted by
production prerequisites. Observe native state before reporting success.

| op | Additional fields | Behavior |
| --- | --- | --- |
| research | type: technology ID 0–43 | Original research handler; one building |
| upgrade | type: upgrade ID 0–60 | Original upgrade handler; one building |
| morph | type: unit ID 0–227 | Unit/building morph handler |
| addon | type, tileX, tileY | Original addon placement handler; one building |
| ability | action | stim, siege, unsiege, burrow, unburrow, cloak, decloak |
| unload | — | Unload all from one transport |
| cancel | action, optional slot | research, upgrade, addon, construction, morph, train; train slot 0–4, omitted means last |
| rally | target, or x/y | Building rally to a visible target or map position |
| repair | target | Owned mechanical target, selected SCVs |
| load | target | One owned transport/bunker and an owned target |

Every request also contains `units`, an array of current native unit handles.
Use `python3 scripts/broodwar-native.py order '<JSON>'`; the CLI supplies the
session and sequence fields. Never copy session credentials into evidence.

Packet layouts were checked against
[BWAPI OrderTypes.h](https://github.com/bwapi/bwapi/blob/d727fed68558c506163048ea889131d8cbb33915/bwapi/BWAPI/Source/BW/OrderTypes.h)
and [GameCommands.cpp](https://github.com/bwapi/bwapi/blob/d727fed68558c506163048ea889131d8cbb33915/bwapi/BWAPI/Source/BWAPI/GameCommands.cpp).
Extended selection uses the already established SCR 32-bit unit handle path.

Validation: 11 Rust tests pass under Miri; desktop/XR viewers and live native
command transitions are recorded in [BROODWAR-M4-CHECKPOINT.md](BROODWAR-M4-CHECKPOINT.md).
Rally uses SCR extended targeted-order opcode 0x61, not a queued building order.
Its layout is pinned by [ShieldBattery commands.rs](https://github.com/ShieldBattery/ShieldBattery/blob/f5eab0dde407c61c4f7d2b0f64c24c960e3ef596/game/src/bw/commands.rs#L341).

Read-only additions: `unit_types` returns native units.dat properties/costs;
`terrain` adds a current-visible creep mask; `state`/`scene` add `dyingUnits` and
`loneSprites` with native image chains. Owned buildings expose `rally` coordinates
and a visibility-filtered target handle. The Pylon union is excluded.

Targeted spells and individual-passenger unload remain unsupported. Wrist
command cards cover common producer relationships and named abilities; their
full technology/requirement coverage and controller gesture acceptance remain
open. units.dat supplies unit costs and properties, not the complete command-card
relationships. See the M4 checkpoint for exact live evidence and remaining gaps.
