# Brood War model coverage

Pinned source: `773c8e8ae8828121e8f37bb74ab82ceb761a45bc`.

110/228 units.dat IDs use 78 distinct converted models. 112 IDs are missing; 6 are parent-rig turret subunits.
110 types passed native renderer checks and four-view exports.

Private artifacts: `runtime/models-m2/{manifest,catalog,provenance}.json`, `runtime/3d-proof/coverage.json`, `runtime/3d-proof/m2/verified-models/`.

Converted does not mean final art approval. Hero geometry aliases and alternate skins are labelled. Missing body textures exclude a model when more than 20% of retained triangles lack diffuse colour. Unsupported transparent effect layers without a colour/mask texture are omitted and recorded; these still need authored effects. Missing normals/specular/emission are listed in the private catalog. Separate birth/death rigs are converted and archived but are not bound automatically to base rigs.

| ID | Unit | Status | Source variant / limitation |
| --- | --- | --- | --- |
| 0 | Marine | converted | base model |
| 1 | Ghost | converted | base model |
| 2 | Vulture | converted | base model |
| 3 | Goliath | converted | base model |
| 4 | Goliath Turret | subunit | Turret geometry belongs to parent rig; not a separate placed model |
| 5 | Siege Tank Tank | converted | base model |
| 6 | Siege Tank Turret | subunit | Turret geometry belongs to parent rig; not a separate placed model |
| 7 | Scv | converted | KMC skin |
| 8 | Wraith | converted | base model |
| 9 | Science Vessel | converted | base model |
| 10 | Gui Montag | converted | Shared geometry with type 32 |
| 11 | Dropship | converted | base model |
| 12 | Battlecruiser | missing | No matching source model in pinned tree |
| 13 | Spider Mine | converted | base model |
| 14 | Nuclear Missile | missing | No matching source model in pinned tree |
| 15 | Civilian | missing | No matching source model in pinned tree |
| 16 | Sarah Kerrigan | converted | Shared geometry with type 1 |
| 17 | Alan Schezar | converted | Shared geometry with type 3 |
| 18 | Schezar Turret | subunit | Turret geometry belongs to parent rig; not a separate placed model |
| 19 | Jim Raynor Vulture | converted | Shared geometry with type 2 |
| 20 | Jim Raynor Marine | converted | Shared geometry with type 0 |
| 21 | Tom Kazansky | converted | Shared geometry with type 8 |
| 22 | Magellan | converted | Shared geometry with type 9 |
| 23 | Edmund Duke Tank | converted | Shared geometry with type 5 |
| 24 | Edmund Duke Tank Turret | subunit | Turret geometry belongs to parent rig; not a separate placed model |
| 25 | Edmund Duke Siege | converted | Shared geometry with type 30 |
| 26 | Edmund Duke Siege Turret | subunit | Turret geometry belongs to parent rig; not a separate placed model |
| 27 | Arcturus Mengsk | missing | No matching source model in pinned tree |
| 28 | Hyperion | missing | No matching source model in pinned tree |
| 29 | Norad Ii | missing | No matching source model in pinned tree |
| 30 | Siege Tank Siege | converted | base model |
| 31 | Siege Tank Siege Turret | subunit | Turret geometry belongs to parent rig; not a separate placed model |
| 32 | Firebat | converted | base model |
| 33 | Scanner Sweep | missing | No matching source model in pinned tree |
| 34 | Medic | missing | No matching source model in pinned tree |
| 35 | Larva | converted | base model |
| 36 | Egg | converted | base model |
| 37 | Zergling | converted | base model |
| 38 | Hydralisk | converted | Webby skin; base Hydralisk absent |
| 39 | Ultralisk | converted | base model |
| 40 | Broodling | missing | No matching source model in pinned tree |
| 41 | Drone | converted | Undead skin; base Drone absent |
| 42 | Overlord | converted | base model |
| 43 | Mutalisk | converted | base model |
| 44 | Guardian | missing | Source mesh converted, but colour textures are absent from pinned repository |
| 45 | Queen | converted | base model |
| 46 | Defiler | converted | base model |
| 47 | Scourge | missing | Source mesh converted, but colour textures are absent from pinned repository |
| 48 | Torrasque | converted | Shared geometry with type 39 |
| 49 | Matriarch | converted | Shared geometry with type 45 |
| 50 | Infested Terran | missing | No matching source model in pinned tree |
| 51 | Infested Kerrigan | missing | No matching source model in pinned tree |
| 52 | Unclean One | converted | Shared geometry with type 46 |
| 53 | Hunter Killer | converted | Shared geometry with type 38; Webby skin; base Hydralisk absent |
| 54 | Devouring One | converted | Shared geometry with type 37 |
| 55 | Kukulza Mutalisk | converted | Shared geometry with type 43 |
| 56 | Kukulza Guardian | missing | Source mesh converted, but colour textures are absent from pinned repository |
| 57 | Yggdrasill | converted | Shared geometry with type 42 |
| 58 | Valkyrie | converted | base model |
| 59 | Cocoon | missing | No textured geometry after unsupported effect layers removed |
| 60 | Corsair | converted | base model |
| 61 | Dark Templar | missing | No matching source model in pinned tree |
| 62 | Devourer | missing | Source mesh converted, but colour textures are absent from pinned repository |
| 63 | Dark Archon | missing | Source colour textures missing on more than 20% of retained triangles |
| 64 | Probe | converted | Dark skin; base Probe absent |
| 65 | Zealot | converted | base model |
| 66 | Dragoon | converted | base model |
| 67 | High Templar | converted | base model |
| 68 | Archon | converted | base model |
| 69 | Shuttle | converted | base model |
| 70 | Scout | converted | base model |
| 71 | Arbiter | converted | base model |
| 72 | Carrier | converted | base model |
| 73 | Interceptor | missing | Source mesh converted, but colour textures are absent from pinned repository |
| 74 | Dark Templar Hero | missing | No matching source model in pinned tree |
| 75 | Zeratul | missing | No matching source model in pinned tree |
| 76 | Tassadar Zeratul | converted | Shared geometry with type 68 |
| 77 | Fenix Zealot | converted | Shared geometry with type 65 |
| 78 | Fenix Dragoon | converted | Shared geometry with type 66 |
| 79 | Tassadar | converted | Shared geometry with type 67 |
| 80 | Mojo | converted | Shared geometry with type 70 |
| 81 | Warbringer | converted | Shared geometry with type 83 |
| 82 | Gantrithor | converted | Shared geometry with type 72 |
| 83 | Reaver | converted | base model |
| 84 | Observer | missing | No matching source model in pinned tree |
| 85 | Scarab | missing | No matching source model in pinned tree |
| 86 | Danimoth | converted | Shared geometry with type 71 |
| 87 | Aldaris | converted | Shared geometry with type 67 |
| 88 | Artanis | converted | Shared geometry with type 70 |
| 89 | Rhynadon | missing | No matching source model in pinned tree |
| 90 | Bengalaas | missing | No matching source model in pinned tree |
| 91 | Cargo Ship | missing | No matching source model in pinned tree |
| 92 | Mercenary Gunship | missing | No matching source model in pinned tree |
| 93 | Scantid | missing | No matching source model in pinned tree |
| 94 | Kakaru | missing | No matching source model in pinned tree |
| 95 | Ragnasaur | missing | No matching source model in pinned tree |
| 96 | Ursadon | missing | No matching source model in pinned tree |
| 97 | Lurker Egg | missing | No matching source model in pinned tree |
| 98 | Raszagal | converted | Shared geometry with type 60 |
| 99 | Samir Duran | converted | Shared geometry with type 1 |
| 100 | Alexei Stukov | converted | Shared geometry with type 1 |
| 101 | Map Revealer | missing | No matching source model in pinned tree |
| 102 | Gerard Dugalle | missing | No matching source model in pinned tree |
| 103 | Lurker | converted | base model |
| 104 | Infested Duran | converted | Shared geometry with type 1 |
| 105 | Disruption Web | missing | No matching source model in pinned tree |
| 106 | Command Center | converted | base model |
| 107 | Comsat Station | missing | Source colour textures missing on more than 20% of retained triangles |
| 108 | Nuclear Silo | converted | base model |
| 109 | Supply Depot | converted | base model |
| 110 | Refinery | converted | base model |
| 111 | Barracks | converted | base model |
| 112 | Academy | converted | base model |
| 113 | Factory | converted | base model |
| 114 | Starport | missing | No matching source model in pinned tree |
| 115 | Control Tower | converted | base model |
| 116 | Science Facility | missing | Source mesh converted, but colour textures are absent from pinned repository |
| 117 | Covert Ops | missing | Source colour textures missing on more than 20% of retained triangles |
| 118 | Physics Lab | missing | No matching source model in pinned tree |
| 119 | Starbase | missing | No matching source model in pinned tree |
| 120 | Machine Shop | converted | base model |
| 121 | Repair Bay | missing | No matching source model in pinned tree |
| 122 | Engineering Bay | converted | base model |
| 123 | Armory | missing | No matching source model in pinned tree |
| 124 | Missile Turret | converted | base model |
| 125 | Bunker | converted | base model |
| 126 | Norad Ii Crashed | missing | No matching source model in pinned tree |
| 127 | Ion Cannon | missing | No matching source model in pinned tree |
| 128 | Uraj Crystal | missing | No matching source model in pinned tree |
| 129 | Khalis Crystal | missing | No matching source model in pinned tree |
| 130 | Infested Command Center | converted | base model |
| 131 | Hatchery | converted | base model |
| 132 | Lair | converted | Authored Hatchery rig morph form: Lair |
| 133 | Hive | converted | Authored Hatchery rig morph form: Hive |
| 134 | Nydus Canal | missing | No matching source model in pinned tree |
| 135 | Hydralisk Den | missing | No matching source model in pinned tree |
| 136 | Defiler Mound | converted | base model |
| 137 | Greater Spire | converted | base model |
| 138 | Queens Nest | converted | base model |
| 139 | Evolution Chamber | converted | base model |
| 140 | Ultralisk Cavern | missing | No matching source model in pinned tree |
| 141 | Spire | converted | base model |
| 142 | Spawning Pool | converted | base model |
| 143 | Creep Colony | converted | base model |
| 144 | Spore Colony | converted | base model |
| 145 | Unused Zerg Building 1 | missing | No matching source model in pinned tree |
| 146 | Sunken Colony | converted | base model |
| 147 | Overmind With Shell | missing | No matching source model in pinned tree |
| 148 | Overmind | missing | No matching source model in pinned tree |
| 149 | Extractor | converted | base model |
| 150 | Mature Chrysalis | missing | No matching source model in pinned tree |
| 151 | Cerebrate | missing | No matching source model in pinned tree |
| 152 | Cerebrate Daggoth | missing | No matching source model in pinned tree |
| 153 | Unused Zerg Building 2 | missing | No matching source model in pinned tree |
| 154 | Nexus | converted | base model |
| 155 | Robotics Facility | converted | base model |
| 156 | Pylon | converted | base model |
| 157 | Assimilator | converted | base model |
| 158 | Unused Protoss Building 1 | missing | No matching source model in pinned tree |
| 159 | Observatory | converted | base model |
| 160 | Gateway | converted | base model |
| 161 | Unused Protoss Building 2 | missing | No matching source model in pinned tree |
| 162 | Photon Cannon | converted | base model |
| 163 | Citadel Of Adun | converted | base model |
| 164 | Cybernetics Core | converted | base model |
| 165 | Templar Archives | converted | base model |
| 166 | Forge | converted | base model |
| 167 | Stargate | converted | base model |
| 168 | Stasis Cell | missing | No matching source model in pinned tree |
| 169 | Fleet Beacon | converted | base model |
| 170 | Arbiter Tribunal | converted | base model |
| 171 | Robotics Support Bay | converted | base model |
| 172 | Shield Battery | converted | base model |
| 173 | Khaydarin Crystal Formation | missing | No matching source model in pinned tree |
| 174 | Temple | missing | No matching source model in pinned tree |
| 175 | Xelnaga Temple | missing | No matching source model in pinned tree |
| 176 | Mineral Field 1 | converted | base model |
| 177 | Mineral Field 2 | converted | base model |
| 178 | Mineral Field 3 | converted | base model |
| 179 | Cave | missing | No matching source model in pinned tree |
| 180 | Cave In | missing | No matching source model in pinned tree |
| 181 | Cantina | missing | No matching source model in pinned tree |
| 182 | Mining Platform | missing | No matching source model in pinned tree |
| 183 | Independent Command Center | missing | No matching source model in pinned tree |
| 184 | Independent Starport | missing | No matching source model in pinned tree |
| 185 | Jump Gate Unused | missing | No matching source model in pinned tree |
| 186 | Ruins | missing | No matching source model in pinned tree |
| 187 | Kyadarin Crystal Formation Unused | missing | No matching source model in pinned tree |
| 188 | Vespene Geyser | missing | Source mesh converted, but colour textures are absent from pinned repository |
| 189 | Warp Gate | missing | No matching source model in pinned tree |
| 190 | Psi Disrupter | missing | No matching source model in pinned tree |
| 191 | Zerg Marker | missing | No matching source model in pinned tree |
| 192 | Terran Marker | missing | No matching source model in pinned tree |
| 193 | Protoss Marker | missing | No matching source model in pinned tree |
| 194 | Zerg Beacon | missing | No matching source model in pinned tree |
| 195 | Terran Beacon | missing | No matching source model in pinned tree |
| 196 | Protoss Beacon | missing | No matching source model in pinned tree |
| 197 | Zerg Flag Beacon | missing | No matching source model in pinned tree |
| 198 | Terran Flag Beacon | missing | No matching source model in pinned tree |
| 199 | Protoss Flag Beacon | missing | No matching source model in pinned tree |
| 200 | Power Generator | missing | No matching source model in pinned tree |
| 201 | Overmind Cocoon | missing | No matching source model in pinned tree |
| 202 | Dark Swarm | missing | No matching source model in pinned tree |
| 203 | Floor Missile Trap | missing | No matching source model in pinned tree |
| 204 | Floor Hatch | missing | No matching source model in pinned tree |
| 205 | Left Upper Level Door | missing | No matching source model in pinned tree |
| 206 | Right Upper Level Door | missing | No matching source model in pinned tree |
| 207 | Left Pit Door | missing | No matching source model in pinned tree |
| 208 | Right Pit Door | missing | No matching source model in pinned tree |
| 209 | Floor Gun Trap | missing | No matching source model in pinned tree |
| 210 | Left Wall Missile Trap | missing | No matching source model in pinned tree |
| 211 | Left Wall Flame Trap | missing | No matching source model in pinned tree |
| 212 | Right Wall Missile Trap | missing | No matching source model in pinned tree |
| 213 | Right Wall Flame Trap | missing | No matching source model in pinned tree |
| 214 | Start Location | missing | No matching source model in pinned tree |
| 215 | Flag | missing | No matching source model in pinned tree |
| 216 | Young Chrysalis | missing | No matching source model in pinned tree |
| 217 | Psi Emitter | missing | No matching source model in pinned tree |
| 218 | Data Disc | missing | No matching source model in pinned tree |
| 219 | Khaydarin Crystal | missing | No matching source model in pinned tree |
| 220 | Mineral Chunk 1 | missing | No matching source model in pinned tree |
| 221 | Mineral Chunk 2 | missing | No matching source model in pinned tree |
| 222 | Vespene Orb 1 | missing | No matching source model in pinned tree |
| 223 | Vespene Orb 2 | missing | No matching source model in pinned tree |
| 224 | Vespene Sac 1 | missing | No matching source model in pinned tree |
| 225 | Vespene Sac 2 | missing | No matching source model in pinned tree |
| 226 | Vespene Tank 1 | missing | No matching source model in pinned tree |
| 227 | Vespene Tank 2 | missing | No matching source model in pinned tree |
