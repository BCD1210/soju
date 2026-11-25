# Diablo II: Resurrected in VR (offline, experimental)

Soju can set up [D2R VR Unleashed](https://github.com/bearstonem/d2r-pcvr) (D2R-PCVR 0.3.31, MPL-2.0), a
first-person VR mod for **offline characters**, and stream it to a headset from your Mac.

```bash
soju vr setup      # once (about 2 minutes, ~125 MB of downloads)
soju vr play       # after connecting the headset or simulator
soju vr play --flat                 # no headset: first person on the Mac screen
soju vr play --flat --view third    # or third person, or --view classic (the game's own camera)
soju vr status
soju vr setup --refresh   # after a D2R update or when the 30-day check appears
soju vr remove --yes
```

In the app: the **VR** badge on the Battle.net Diablo II: Resurrected card (or **VR and First Person** in its details
or right-click menu) opens a three-step sheet: the headset app, Set Up, Connect and Play in VR. Its lower part,
**No headset? Play on this Mac**, starts the same copy on the Mac screen in first person, third person or the classic
camera, with mouse look and W A S D.

## On the Mac screen (no headset)

The mod's flat mode (`[mode] platform = 0` in `d2r_pcvr.ini`, view in `flat_view`: 1 classic, 2 third person,
3 first person) is set before each start, because Mac keyboards send F1 - F3 as brightness keys unless fn is held
(fn+F1 / fn+F2 / fn+F3 still switch in the game). Checked 2026-11-25 on D2R 3.4.95032: first and third person in the
Rogue Encampment, offline character select with the mouse.

On the Mac screen the game keeps your own resolution and window mode (in VR it is fixed at windowed 1920x1080).

NVIDIA DLSS works in the copy through Apple's MetalFX bridge, like normal D2R, when the engine has Apple's NGX shim.
Two things were in the way and are fixed by Soju (2026-11-25, D2R 3.4.95032, GPTK 3.0, M4 Pro):

- The mod runs the game under ReShade, which hands it wrappers of the D3D12 device and command lists. Apple's shim
  treats the command list as its own object, so it crashed in `NVSDK_NGX_D3D12_CreateFeature` and the game hung at the
  title. Soju's `nvngx.dll` (`tools/ngx-unwrap`, built into the `vr-mod-v2` release) sits in front of the shim in the
  copy's system32 and passes it the original objects; the shim itself is loaded as `nvngx_metalfx.dll`.
- The mod reads the depth buffer from the part DLSS draws into (2/3 of the screen at Quality), found from the depth
  buffer binds. Under D3DMetal the game binds it with D3D12 render passes, which the mod did not watch, so fog and sky
  were drawn over a 2/3 box. The mod patch (`patches/d2r-pcvr`) now watches render passes too.

Measured in the Rogue Encampment, first person, fog and sky on: at 3440x1440 full screen 51 - 66 fps without DLSS and
63 - 74 fps with DLSS Quality (MetalFX 2293x960 -> 3440x1440). In a 1920x1080 window the GPU is not the limit and DLSS
gives no gain (about 130 fps either way). The headset path with DLSS is not checked yet.

## What it does

- Makes an APFS copy of your Battle.net bottle in `~/.battlenet-macos/d2r-vr/bottle`. The copy shares disk space with
  the original until files change, and **your Battle.net bottle is never modified**. Normal Play still starts the stock
  game.
- Downloads, each pinned by size and SHA-256 in `resources/d2r-vr.json`: the mod's release ZIP (GitHub), D2RLoader
  1.3.1-beta (d2rloader.net), ReShade 6.8.0 (reshade.me; only `ReShade64.dll` is extracted, the setup never runs),
  Windows embeddable Python 3.12 (python.org) for the mod's own installer, Soju's VR runtime, and the patched mod DLL.
- The VR runtime ([vr-runtime-v1](https://github.com/BCD1210/soju/releases/tag/vr-runtime-v1)) is built by
  `scripts/build-vr-runtime.sh` from pinned commits: the OXRSys OpenXR runtime for macOS (MPL-2.0) and a wineopenxr
  bridge (LGPL-2.1-or-later) that hands the game's D3D12 eye images to the runtime as Metal textures.
- Three changes inside the copy make the mod run on Soju's engine (D3DMetal): the game folder's `d3d12.dll` is renamed
  so D3D12 loads where ReShade watches for it, the windowed resolution is fixed at 1920x1080 so the game never resizes
  its swapchain, and one jump in `ReShade64.dll` skips work on a resource D3DMetal has already destroyed (pinned
  before/after hashes).
- `soju vr play` starts `D2RLoader.exe -mod D2R-PCVR`. `-mod` keeps D2R offline: no Battle.net game servers.

## Headset

The game streams to a headset running the **OXRSys client** (Meta Quest / PICO, sideloaded; see the
[OXRSys Quest guide](https://github.com/anoshmisiosdev/oxrsys/blob/main/docs/platforms/quest.md)), or to the OXRSys
Simulator on the Mac for testing. The Mac and the headset need to be on the same network (or USB with adb).

Status (November 2026): verified on M4 Pro / macOS 26.5 with the OXRSys Simulator: first-person view in Act 1 at
72 fps. **Not yet tested on a real headset.** Physical Quest controller input therefore has not been checked.

D2R 3.4.95032 (the Battle.net build since November 2026) is accepted too, listed under `extra_builds` in
`resources/d2r-vr.json`. Checked 2026-11-25 on the same Mac with the simulator: setup, the title, the VR character
list and first-person play in the Rogue Encampment at 58-74 fps, with the same mod release; its game hooks and
combat checks all found their code. One thing is not found on this build: the see-through wall fade, so walls fade
as the game has them. Setup also answers D2RLoader's "This D2R version hasn't been fully tested yet" window for
that version (`d2rloader/data/loader-state.json`), which nobody in a headset could press.

## Create an offline character inside VR

Connect the OXRSys headset/client or Simulator **before `soju vr play`**. If the
runtime only becomes discoverable when the game starts, have the client ready and
connect as soon as it appears, before advancing the title screen. Opening the
simulator window alone is insufficient: press **Connect** so it sends tracked head
poses. The menu cannot anchor without tracking.

1. Press **A** at the title and release it. The character list opens in controller
   mode. On the VR **Characters** panel, move the **left stick right** to highlight
   **New**, then press **A**. Up/down still selects an existing character; left
   selects **Play**. Start remains a shortcut for New.
2. Move the **left stick left/right** to choose a class, then press **A** and release
   it. The VR **Character Name** keyboard opens automatically.
3. Move the **left stick** across the letters and press **A** for each letter.
   **X** erases one character; **Y** switches letter case. Select **Create** at the
   lower right and press **A** to create the character and enter its game.
   D2R validates the name, including length and duplicates. An invalid name keeps
   the screen open so you can correct it. No desktop click or `--flat` launch is needed.
4. **B** backs out of name entry; another B returns from creation to the character
   list. On the list, B opens the native exit confirmation; B cancels it.
   Release each press between screens. A held accept cannot select the next
   screen's default item. Physical keyboard input remains available through D2R's
   normal text field when the keyboard is routed to the game.

For the local lab simulator build: **I/K/J/L** = left stick up/down/left/right;
**B** = controller A/confirm; **N** = controller B/back; **Escape** = Start/New;
**Z** = controller X/erase; **X** = controller Y/case; **Space/V** = right/left trigger.
Keep the simulator frontmost while using these keys and release each press.
**Tab** toggles right-hand mouse aim; press it again to reset the hand aim and
return to mouse head look. The existing backtick toggle aims the left hand.
These keyboard mappings are lab additions; the stock OXRSys app may differ.
On Quest use the physical left thumbstick and A/B/X/Y buttons.

The PC build's controller name field did not open an on-screen keyboard when
selected or accepted in the lab. The mod supplies its own VR keyboard and routes
its keys to the native name field; it does not create saves or bypass game validation.
The simulator created the Necromancer `sojunecro` with controller input and entered
stereo gameplay. An empty character list was also tested: New → Warlock →
`sojufresh` → stereo gameplay, using only simulator controller keys. B was checked
in the list exit confirmation, class selection, and name entry. Wait for class
selection animations to finish before sending the next action. The OXRSys menu
includes a black projection background so its runtime streams the menu quads. The virtual pad stays connected with neutral
controls while loading pauses XR publication, avoiding the reconnect dialog.

After restarting D2R, disconnect the simulator's old session and Search/Connect
again; an old frozen image with RECV 0 does not mean it connected to the new game.
Real Quest testing remains outstanding. A separate existing body-calibration
font-weight warning appeared in the lab build; this work does not qualify that
calibration UI or extended gameplay.

## Patched mod distribution

Setup verifies the additional `mod_patch` DLL pin in `resources/d2r-vr.json`, then
passes that DLL to the upstream installer, which backs up an existing plugin.
The MPL-2.0 source patch, license, exact upstream source commit, and rebuild guide
are in [patches/d2r-pcvr](../patches/d2r-pcvr/README.md). A source notice and patch
are also placed in the installed mod's `soju-source` directory.

The `vr-mod-v1` release is staged locally for this change and is **not published**.
Publish its pinned DLL and corresponding source before distributing this Soju
version. Setup deliberately cannot substitute an unpatched or unchecked DLL.

## Notes

- Use **offline characters** only. Characters you make in VR live in the copy; `--refresh` keeps them.
- D2R requires an online sign-in within the last 30 days even for offline play. If it says so, play D2R online once
  from Battle.net, then run `soju vr setup --refresh`.
- Setup accepts D2R 3.3.93847 (the mod release's own build) and 3.4.95032 (checked by Soju, above), and refuses
  other builds.
- Modding is your choice and the mod is not Blizzard's. Online play is never used by this mode.
