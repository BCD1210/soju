# Remastered reconstruction coverage

This is an experimental reconstruction inventory, not completed Remastered-quality art.
0 / 228 types pass the complete gate. SCEvo stays the temporary fallback.

Private pilot results: `runtime/models-remastered/0/rigged-e32/fit/` and `runtime/models-remastered/7/rigged-dense/seeded/`; each has `validation.json` and `comparisons/{00..16}.png`.
IoU/SSIM below are the minimum over all 17 directions; — means not measured.
Full source inventory and limitations: `runtime/3d-proof/remastered-coverage.json`.

| Type | Name | Frames | Status | Min IoU | Min SSIM | Reason |
| --- | --- | ---: | --- | ---: | ---: | --- |
| 0 | Marine | 229 | failed-gate | 0.7900 | 0.3254 | Rigged fit below per-direction thresholds; clips retained plus partial death retarget; live checks separate |
| 1 | Ghost | 229 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 2 | Vulture | 17 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 3 | Goliath | 170 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 4 | Goliath Turret | 187 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 5 | Siege Tank Tank | 51 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 6 | Siege Tank Turret | 17 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 7 | Scv | 51 | failed-gate | 0.7937 | 0.2777 | Rigged fit below per-direction thresholds; clips retained plus partial death retarget; live checks separate |
| 8 | Wraith | 17 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 9 | Science Vessel | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 10 | Gui Montag | 170 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 11 | Dropship | 17 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 12 | Battlecruiser | 17 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 13 | Spider Mine | 12 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 14 | Nuclear Missile | 17 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 15 | Civilian | 136 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 16 | Sarah Kerrigan | 221 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 17 | Alan Schezar | 170 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 18 | Schezar Turret | 187 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 19 | Jim Raynor Vulture | 17 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 20 | Jim Raynor Marine | 229 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 21 | Tom Kazansky | 17 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 22 | Magellan | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 23 | Edmund Duke Tank | 51 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 24 | Edmund Duke Tank Turret | 17 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 25 | Edmund Duke Siege | 6 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 26 | Edmund Duke Siege Turret | 102 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 27 | Arcturus Mengsk | 17 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 28 | Hyperion | 17 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 29 | Norad Ii | 17 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 30 | Siege Tank Siege | 6 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 31 | Siege Tank Siege Turret | 102 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 32 | Firebat | 170 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 33 | Scanner Sweep | 229 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 34 | Medic | 230 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 35 | Larva | 85 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 36 | Egg | 24 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 37 | Zergling | 296 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 38 | Hydralisk | 297 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 39 | Ultralisk | 265 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 40 | Broodling | 204 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 41 | Drone | 391 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 42 | Overlord | 68 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 43 | Mutalisk | 85 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 44 | Guardian | 119 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 45 | Queen | 187 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 46 | Defiler | 289 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 47 | Scourge | 85 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 48 | Torrasque | 265 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 49 | Matriarch | 187 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 50 | Infested Terran | 246 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 51 | Infested Kerrigan | 272 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 52 | Unclean One | 289 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 53 | Hunter Killer | 297 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 54 | Devouring One | 296 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 55 | Kukulza Mutalisk | 85 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 56 | Kukulza Guardian | 119 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 57 | Yggdrasill | 68 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 58 | Valkyrie | 17 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 59 | Cocoon | 23 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 60 | Corsair | 85 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 61 | Dark Templar | 306 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 62 | Devourer | 170 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 63 | Dark Archon | 27 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 64 | Probe | 17 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 65 | Zealot | 228 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 66 | Dragoon | 415 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 67 | High Templar | 296 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 68 | Archon | 27 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 69 | Shuttle | 17 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 70 | Scout | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 71 | Arbiter | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 72 | Carrier | 17 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 73 | Interceptor | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 74 | Dark Templar Hero | 306 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 75 | Zeratul | 306 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 76 | Tassadar Zeratul | 27 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 77 | Fenix Zealot | 228 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 78 | Fenix Dragoon | 415 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 79 | Tassadar | 296 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 80 | Mojo | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 81 | Warbringer | 153 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 82 | Gantrithor | 17 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 83 | Reaver | 153 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 84 | Observer | 17 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 85 | Scarab | 17 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 86 | Danimoth | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 87 | Aldaris | 296 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 88 | Artanis | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 89 | Rhynadon | 195 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 90 | Bengalaas | 212 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 91 | Cargo Ship | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 92 | Mercenary Gunship | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 93 | Scantid | 212 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 94 | Kakaru | 214 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 95 | Ragnasaur | 161 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 96 | Ursadon | 144 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 97 | Lurker Egg | 20 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 98 | Raszagal | 85 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 99 | Samir Duran | 229 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 100 | Alexei Stukov | 229 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 101 | Map Revealer | 4 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 102 | Gerard Dugalle | 17 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 103 | Lurker | 595 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 104 | Infested Duran | 229 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 105 | Disruption Web | 11 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 106 | Command Center | 6 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 107 | Comsat Station | 2 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 108 | Nuclear Silo | 2 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 109 | Supply Depot | 2 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 110 | Refinery | 5 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 111 | Barracks | 9 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 112 | Academy | 2 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 113 | Factory | 7 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 114 | Starport | 6 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 115 | Control Tower | 2 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 116 | Science Facility | 6 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 117 | Covert Ops | 2 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 118 | Physics Lab | 7 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 119 | Starbase | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 120 | Machine Shop | 5 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 121 | Repair Bay | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 122 | Engineering Bay | 6 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 123 | Armory | 2 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 124 | Missile Turret | 3 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 125 | Bunker | 2 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 126 | Norad Ii Crashed | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 127 | Ion Cannon | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 128 | Uraj Crystal | 17 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 129 | Khalis Crystal | 17 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 130 | Infested Command Center | 6 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 131 | Hatchery | 6 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 132 | Lair | 5 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 133 | Hive | 5 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 134 | Nydus Canal | 5 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 135 | Hydralisk Den | 3 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 136 | Defiler Mound | 4 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 137 | Greater Spire | 4 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 138 | Queens Nest | 4 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 139 | Evolution Chamber | 3 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 140 | Ultralisk Cavern | 3 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 141 | Spire | 4 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 142 | Spawning Pool | 4 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 143 | Creep Colony | 4 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 144 | Spore Colony | 4 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 145 | Unused Zerg Building 1 | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 146 | Sunken Colony | 34 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 147 | Overmind With Shell | 4 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 148 | Overmind | 4 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 149 | Extractor | 4 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 150 | Mature Chrysalis | 4 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 151 | Cerebrate | 5 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 152 | Cerebrate Daggoth | 5 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 153 | Unused Zerg Building 2 | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 154 | Nexus | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 155 | Robotics Facility | 20 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 156 | Pylon | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 157 | Assimilator | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 158 | Unused Protoss Building 1 | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 159 | Observatory | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 160 | Gateway | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 161 | Unused Protoss Building 2 | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 162 | Photon Cannon | 4 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 163 | Citadel Of Adun | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 164 | Cybernetics Core | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 165 | Templar Archives | 2 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 166 | Forge | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 167 | Stargate | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 168 | Stasis Cell | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 169 | Fleet Beacon | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 170 | Arbiter Tribunal | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 171 | Robotics Support Bay | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 172 | Shield Battery | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 173 | Khaydarin Crystal Formation | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 174 | Temple | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 175 | Xelnaga Temple | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 176 | Mineral Field 1 | 4 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 177 | Mineral Field 2 | 4 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 178 | Mineral Field 3 | 4 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 179 | Cave | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 180 | Cave In | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 181 | Cantina | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 182 | Mining Platform | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 183 | Independent Command Center | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 184 | Independent Starport | 34 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 185 | Jump Gate Unused | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 186 | Ruins | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 187 | Kyadarin Crystal Formation Unused | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 188 | Vespene Geyser | 8 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 189 | Warp Gate | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 190 | Psi Disrupter | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 191 | Zerg Marker | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 192 | Terran Marker | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 193 | Protoss Marker | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 194 | Zerg Beacon | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 195 | Terran Beacon | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 196 | Protoss Beacon | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 197 | Zerg Flag Beacon | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 198 | Terran Flag Beacon | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 199 | Protoss Flag Beacon | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 200 | Power Generator | 7 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 201 | Overmind Cocoon | 4 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 202 | Dark Swarm | 10 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 203 | Floor Missile Trap | 12 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 204 | Floor Hatch | 10 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 205 | Left Upper Level Door | 10 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 206 | Right Upper Level Door | 10 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 207 | Left Pit Door | 10 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 208 | Right Pit Door | 10 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 209 | Floor Gun Trap | 12 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 210 | Left Wall Missile Trap | 7 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 211 | Left Wall Flame Trap | 11 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 212 | Right Wall Missile Trap | 7 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 213 | Right Wall Flame Trap | 11 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 214 | Start Location | 1 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 215 | Flag | 6 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 216 | Young Chrysalis | 17 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 217 | Psi Emitter | 7 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 218 | Data Disc | 17 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 219 | Khaydarin Crystal | 17 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 220 | Mineral Chunk 1 | 17 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 221 | Mineral Chunk 2 | 17 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 222 | Vespene Orb 1 | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 223 | Vespene Orb 2 | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 224 | Vespene Sac 1 | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 225 | Vespene Sac 2 | 34 | not-reconstructed | — | — | Directional/action layout not yet verified; pilot fidelity gate has not passed |
| 226 | Vespene Tank 1 | 2 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
| 227 | Vespene Tank 2 | 2 | insufficient-multiview | — | — | Non-directional image or fewer than 17 frames; no measured 17-direction static pose |
