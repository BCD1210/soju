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

Validation: Windows DLL compiles; six Rust tests pass under Miri with strict
provenance and symbolic alignment. These exercise packet validation and event
history, not foreign game execution. Live verification of the new endpoints
remains pending: the restarted original client is listening but not advancing
its frame hook (menus or paused). Do not infer gameplay success from these tests.

Targeted spells, individual-passenger unload, full producer/requirement command
cards and controller bindings for these extensions are not implemented yet.
`units.dat` provides unit properties and costs; it does not alone contain the
complete command-card/producer/technology targeting relationships.
