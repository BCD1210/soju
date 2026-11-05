# Brood War native commander development

The current implementation runs **Blizzard's original StarCraft.exe**, version
1.23.10.13515, in a dedicated Wine bottle. Samase 0.10.28-64 loads the Soju Rust
module inside that process. No replacement engine is used by this path.

The default desktop/OpenXR Metal renderer uses native HD relief meshes and
sprite chains, original-client positions/headings, and map-derived 3D terrain.
The SceneKit fallback supports independent rigged models and live GLB-pack
replacement. See [BROODWAR-ASSET-INTEGRATION.md](BROODWAR-ASSET-INTEGRATION.md)
for that delivery contract, installation, rollback and verification. Dots is
producing replacement models. Physical-headset and all-map/combat coverage
remain unverified.

`soju bw-vr` now launches this original-client OpenXR path. The earlier OpenBW /
placeholder-mesh source remains under tools/broodwar_vr for reference.

## Native client commands

Prerequisites on the development Mac: the existing Soju CrossOver26 engine,
an owned installed StarCraft client, Rust with x86_64-pc-windows-gnu, and
/opt/homebrew/bin/x86_64-w64-mingw32-gcc.

```sh
bash scripts/soju bw-native setup
bash scripts/soju bw-native build
bash scripts/soju bw-native play --menu  # normal account/HD settings, then single-player
bash scripts/soju bw-native play         # direct private offline map (after stopping any existing session)
bash scripts/soju bw-native state
bash scripts/soju bw-native status
bash scripts/soju bw-native stop
```

Setup uses an APFS clone of the resolved source bottle and isolates its Documents
directory. It pins and checks the Samase executable hash, installs Wine font
fallbacks required by Samase's debug overlay, and leaves the original executable
unchanged. Runtime files are in ~/.local/share/soju/broodwar-native.
The launcher is deliberately restricted to the exact executable tested here;
a different game build needs a compatibility test before acceptance.

Samase binary source: the maintained
[sezam distribution](https://github.com/BoneHorror/sezam/tree/bd2368cb2b64bc427d7a7e0f15e58c119cb7ef8a/bin).
API declarations and structure dependencies are pinned in Cargo.toml/Cargo.lock.
Samase's loader is not open source; the Soju module is separate.
No Blizzard executable, game asset, or third-party binary is committed here.

## Protocol

The DLL listens only on UDP 127.0.0.1:8795 and requires the generated session token.
All game reads and commands execute from Samase's post-simulation frame callback.
Menus, dialogs and paused games can therefore cause a request timeout.
Only offline single-player, non-replay sessions accept requests.

The snapshot reports original engine frames, resources, visible units, health,
positions, orders, completion, and the current main image/frame.
Unit handles are validated against the current active list and owner.
Commands reject hidden targets, stale handles, invalid coordinates, unsupported
producers and duplicate/out-of-order sequence numbers.

```sh
bash scripts/soju bw-native order '{"op":"move","units":["HANDLE_FROM_STATE"],"x":3500,"y":1800}'
bash scripts/soju bw-native order '{"op":"gather","units":["SCV_HANDLE"],"target":"MINERAL_HANDLE"}'
bash scripts/soju bw-native order '{"op":"follow","units":["MARINE_HANDLE"],"target":"SCV_HANDLE"}'
bash scripts/soju bw-native order '{"op":"build","units":["SCV_HANDLE"],"type":109,"tileX":117,"tileY":60}'
bash scripts/soju bw-native order '{"op":"train","units":["COMMAND_CENTER_HANDLE"],"type":7}'
```

Move, attack-move, gather, follow and stop use the original order function.
Build/train use original selection and command packets, preserving native cost,
requirements and construction/production timing. A queued acknowledgement does
not mean the game accepted a build location or completed production; observe
the resulting state. Current building support is Command Center, Supply Depot
and Barracks; training support is SCV and Marine.

## Actual gameplay verification, 2026-11-04 UTC

The native test used a private UMS map derived locally from the installed
Enslavers01 map. It retained terrain, resources, player metadata and original
assets, removed campaign/AI triggers, and placed a Command Center, two SCVs and
two Marines. No resources were granted by the bridge.

Observed on the actual Blizzard executable with Samase API 44:

- Movement changed an SCV from (3970,1694) to (3839,1707).
- Mining raised the bank from 0 to 32 in initial checking; the recorded repeat
  raised it from 88 to 96.
- A Marine's gather request and a stale unit handle were rejected.
- Native production increased SCVs from two to three.
- A Supply Depot was placed at (3792,1952), progressed from 50.75 HP to 500 HP,
  and reached the native completed state.
- A Marine accepted native Follow order 49 and moved toward the commanded SCV.

The verification script is tools/broodwar_native/verify.py.
Detailed observations are in the local runtime's native-verification.json.
It requires the prepared test map and observes actual state, not acknowledgements
alone. It is not a test of all maps, campaigns, combat, Battle.net or VR.

tools/broodwar_native/make_fixture.py creates the local test map using an
explicitly supplied StormLib shared library and owned source map. The tested
source is Maps/campaign/(1)Enslavers01.scm; random-race melee maps are not validated
for this direct UMS startup path.

## Earlier sprite renderer and HD entitlement checks (historical)

The following describes the earlier renderer and its verification, not the active
3D asset quality or its acceptance. Current work is documented at the end.

## Earlier HD graphics and sprite rendering (historical)

The launcher no longer passes --no-hd. HD availability must come from the
normal Remastered entitlement/settings; do not force an entitlement flag.
After the user's Remastered change, the normal menu showed HD Graphics enabled,
along with Environment Effects, Real-Time Lighting, Smooth Unit Turning and
Shadow Stacking. The private UMS test map was then opened through Single Player
> Brood War > Play Custom with HD terrain, units and UI visible. The native
bridge still returned live original-client state in that session. The HD run
passed mining (0 to 8 minerals), native SCV production (2 to 3), a 100-mineral
Supply Depot charge (102 to 2), construction to 500 HP/completed, and Marine
Follow movement. Evidence: runtime/remastered-hd-verification.json. This validates
HD gameplay commands only; it does not validate a VR renderer or headset.

Use `play --menu` when account/HD setup is needed; it does not force SD or bypass
entitlement. A listener check prevents a second launch from rotating the current
session token while the first game is paused or in menus. `status` distinguishes
an occupied bridge port from an active simulation response.

The bridge now also exposes bounded image-chain metadata through `scene` and
player-specific explored/visible tile masks through `terrain`. The native SceneKit/
Metal viewer consumes the compact main-image state (not unused overlay chains) and
uses local CASC HD diffuse atlases, actual animation frames, original
map tiles and fog masks. It has a desktop walking camera, selection, move, mine,
follow and SCV train inputs. A desktop right-click moved two actual SCVs from
(3731,1878)/(3619,1786) to (3652,1849)/(3629,1828); evidence is in
runtime/scene-ui-verification.json. Walking was visually checked in the same scene.

That earlier sprite viewer rendered the main image only; turret/overlay/shadow composition,
team colors beyond the tested white player, RTL material layers, terrain elevation,
creep deltas and projectiles remain incomplete. Static terrain assets must match the
current map. This is not a fully faithful visual replacement for the original renderer.
Classic sprites can support a 2.5D walking view, but they contain no back-facing
geometry or complete 3D unit/building models. A 3D camera change alone cannot
recover those models. The native bridge must remain the sole simulation authority.

## Native scene development

Build/launch the prepared desktop scene:

```sh
python3 tools/broodwar_native/build_scene.py
bash scripts/soju bw-native scene
```

WASD walks the viewer; dragging looks around; 1/2/3 select workers/Marines/base;
right-click explored ground moves selected units; G mines visible minerals;
F follows a worker; T trains an SCV; B places a Supply Depot on the next ground
click; R recenters tracked XR position and heading. Game commands use the native bridge.
The active view uses rigged 3D models. L reloads model definitions; X stops selected units.

Local assets are prepared with:

```sh
python3 tools/broodwar_native/scene_assets.py --casc-lib PATH_TO_CASCLIB --storm PATH_TO_STORMLIB --map YOUR_LOCAL_MAP
```

CascLib was built from commit 6b889ec8f421cbc9baccec296c582362ab917887.
ANIM/DDS-GRP parsing and the mode-512 HD alias are based on the public
[Animosity format implementation](https://github.com/neivv/animosity).
Image 275 correctly resolves to original HD image 63. No extracted game art is
committed or redistributed.

```sh
bash scripts/soju bw-native scene-build --xr
bash scripts/soju bw-native xr-probe
bash scripts/soju bw-native scene --xr
```

The XR build uses the existing tested x86_64 Soju OpenXR SDK/loader/runtime from
the D2R development environment. The build helper accepts explicit SDK/loader
paths. The probe checks the loader, system and runtime Metal device without
creating a session. The launcher refuses to start streaming if another VR session
owns port 9946. It does not terminate that other session.

The OpenXR path uses two Metal eye swapchains, predicted view poses/FOV, a shared
head center for billboards and a shared timestamp for both eyes. Camera changes
are committed without animation and flushed before rendering. Initial origin is
calibrated only from tracked poses, including X/Z and heading. The desktop mirror
copies the left eye into app-owned textures before releasing the runtime image,
then preserves its aspect ratio. It is a low-rate preview, not a second XR camera.

Earlier controller mapping (superseded by the M19 controls below):

- Left trigger cycles SCVs, Marines and Command Centers.
- Left stick walks; right stick snaps the body heading by 30 degrees.
- Right grip cycles Point/move, Mine, Follow SCV, Train SCV and Build depot.
- Right trigger confirms the current action. Point mode selects owned units,
  gathers targeted minerals or moves the selection to explored ground.
- A green marker shows the ray intersection; the in-view panel shows selection,
  resources, action and the last command result.

The tested OXRSys Simulator uses V for the left trigger, Space for the right trigger,
G for right grip, I/J/K/L for the left stick, U/O for right-stick turns and arrows
for head rotation. Its F key toggled fullscreen on the running build, so it is not
required for the Soju control path.

## Earlier sprite XR simulator verification — 2026-11-03 America/Los_Angeles

Observed through simulator input -> OpenXR actions -> native bridge -> original
Blizzard simulation, with separate state reads:
- Two SCVs moved from (3431,1923)/(3405,1918) to (3706,1859)/(3690,1882).
- The Mine menu action increased minerals from 0 to 40.
- Train SCV increased the worker count from two to three.
- Build depot targeted tile (117,60); the original game created the building at
  (3792,1952), with 54.5 HP initially and 500 HP/completed later.
- Two Marines entered Follow order 49 and moved from (3432,1830)/(3442,1810)
  to (3371,1900)/(3394,1915).
- Stick locomotion changed body X/Z from (115,61.5) to (115.7823,60.1950);
  right-stick input changed yaw from -0.54 to -1.0636 radians.
- Streaming showed approximately 72 received frames/second with no displayed
  loss/drop errors during the observed run. This is not a latency/comfort benchmark.

Evidence is in the runtime's xr-gameplay-{move,mine,train,build-start,build-complete,
follow,locomotion}.json and xr-gameplay-proof.png. These observations cover the
prepared small offline test map. They do not establish campaign, combat, every
unit type, all-map compatibility or physical-headset usability.

Large snapshots use SBW1 transport chunks rather than exceeding macOS's default
UDP datagram limit. Each datagram is at most 8192 bytes, with a 16-byte little-endian
header: magic, response ID (u32), index (u16), count (u16), total bytes (u32).
Receivers bound the response to 512 KiB, reject mixed/duplicate/malformed chunks,
and apply only complete snapshots. Small JSON responses remain compatible.
The transport regression tests cover >9 KiB, 100 KiB and 512 KiB responses,
reordering, missing chunks, mixed IDs and malformed bounds in Python and Swift.
Run: `python3 tools/broodwar_native/test_wire.py`.

After the transport change, a fresh original-client regression again passed mining,
SCV production, a 100-mineral Depot charge/completion and Follow. Actual 8199-byte
scene JSON arrived as 8192/39-byte datagrams and decoded successfully; both native
Swift builds also decoded the live scene. Fresh XR controller Mine raised minerals
from 98 to 202. Evidence: final-xr-verification.json and native-verification.json.

Explored fog darkens opaque terrain instead of making the ground transparent, and
the fog query covers the whole rendered terrain patch. Remaining limits include
the main-image-only renderer, a 240-visible-unit snapshot cap (the viewer reports
an error instead of applying a partial snapshot), map-specific prepared assets and
at that stage no complete volumetric 3D models. These results describe the earlier
2.5D commander prototype, before the rigged-model integration below.


## Current true-3D work, 2026-11-04 UTC

The active NativeScene.swift no longer creates unit SCNPlanes or rotates units
around the camera. Models3D.swift imports per-unit rigs through GLTFKit2. Native
facing is exported by the DLL; Marine/SCV model correction is +pi, followed by
-facing*2pi/256 on the unit root.

Prepared external model types: Marine 0, SCV 7, Command Center 106, Depot 109,
minerals 176/177/178 and geyser 188. The geyser material set is incomplete.
Missing types are reported rather than replaced with billboards or boxes.
Marine and SCV asset fidelity is explicitly below the user's acceptance target.
Do not call these initial assets Remastered-quality or claim all-unit coverage.

prepare_models.py pins the creator source and stores provenance in the runtime.
convert_models.py verifies nonzero 3D bounds and triangle geometry and converts
embedded DDS images to standard PNG. prepare_3d.py reproduces the external tools.
No external game art is committed. Models are in runtime/models-3d.

Terrain uses native VF4 minitile flags (Walkable 1, Middle 2, High 4, Ramp 16).
Plateaus retain separate vertices and actual vertical cliff faces. Only identified
walkable ramp regions interpolate between adjacent levels. Height step 2 is a
visual scale convention, not a measured original-game meter value. Wider tileset
coverage and ramp topology still require validation.

New evidence: runtime/3d-proof has four-view geometry renders and geometry.json.
A live 3D desktop run again passed mining 0->8, SCVs 2->3, Depot cost 102->2,
completion 51.5->500 HP, and Marine Follow order 49 movement. This run is recorded
in native-verification.json. The 3D XR scene is visible in OXRSys Simulator.
Previous xr-gameplay JSON files are from the sprite renderer and must not be
represented as completed regression evidence for the 3D renderer.

Animation channels now bind directly to each instance's own nodes, including
Blender bone names containing dots. ARM64 desktop and x86_64 XR checks verify
independent skeletons, moving walk bones and zero changes in the second instance.
The revised proof harness samples system time, matching live playback; a separate
comparison also matched the original isolated glTF animation's bone transforms.

Model types, paths, size, orientation and animation aliases are configured in JSON.
Live Marine/SCV pack installation and rollback passed while the same original game
kept advancing. Native SCV production created a fourth worker and automatically
spawned its 3D model. Follow, mining and a second completed Supply Depot were also
observed in the current 3D viewer. See the integration guide for exact evidence
and the remaining limits; prior sprite controller results are still historical.

The user's Dots/Blender assignment is in DOTS-BROODWAR-ASSETS.md.
Blender 4.5.15 LTS is installed privately at runtime/blender/Blender.app and its
vendor SHA-256 was verified. Owned HD reference crops are in runtime/asset-reference.
The Dots conversation was read directly: Blender export testing is complete and
Marine/SCV geometry and new rigs are in progress. No final replacement GLBs had
been delivered at the last check. Asset production and visual acceptance remain
separate from the completed model-loading integration.

## M19 command table and unit view

The command table frames the whole map at an initial width of 1.8 metres.
Fog retains the original game's visibility. Picking and orders use native map
coordinates in all views. Leaving the table restores the saved ground pose.

| Action | Controller | Desktop |
|---|---|---|
| Ground / command table | Left grip | V |
| Table scale | Hold right grip + left stick vertical | [ / ] |
| Table height | Hold right grip + left stick horizontal | - / = |
| Rotate table | Right stick horizontal | , / . |
| Recenter table | Hold right grip, press left grip | R |
| Possess owned unit under pointer | Hold right grip, press right trigger | Tab (pointer, then selected unit) |
| Exit unit view | Right grip | Escape or Tab |
| Move possessed unit | Left stick, relative to gaze | WASD, relative to view |
| Attack from unit view | Right trigger | Right-click |
| Snap turn in unit view | Right stick horizontal | Option-drag to look |

Action cards remain available while possessing; a pending card target takes
precedence over the normal attack action. Movement and attacks call the original
native command handlers. No direct unit movement is performed by the viewer.
The camera follows position smoothly without following subsequent unit turns.
Entering a transport, burrowing, death, loss of ownership or disappearance ends
possession and restores the previous ground or table view. Buildings and hidden,
transported, burrowed or dead units cannot be entered. Air units use an elevated
viewpoint. Metal rendering fades in after a view change and adds a peripheral
vignette during rapid follow movement.

Qualification status and remaining limits are tracked in
[BROODWAR-M19-CHECKPOINT.md](BROODWAR-M19-CHECKPOINT.md). Offline runtime checks
are separate from simulator controller and physical-headset qualification.
