# Diablo II: Resurrected in VR (offline, experimental)

Soju can set up [D2R VR Unleashed](https://github.com/bearstonem/d2r-pcvr) (D2R-PCVR 0.3.31, MPL-2.0), a
first-person VR mod for **offline characters**, and stream it to a headset from your Mac.

```bash
soju vr setup      # once (about 2 minutes, ~125 MB of downloads)
soju vr play       # after connecting the headset or simulator
soju vr play --flat   # optional desktop mode
soju vr status
soju vr setup --refresh   # after a D2R update or when the 30-day check appears
soju vr remove --yes
```

In the app: right-click Diablo II: Resurrected in the library, then **Set Up VR (offline)** and **Play in VR**.
**Create Offline Character (without VR)** opens the same copy on the Mac screen.

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
72 fps. **Not yet tested on a real headset.** Controller input therefore has not been checked.

## Create an offline character inside VR

Connect the OXRSys headset/client or Simulator **before `soju vr play`**. If the
runtime only becomes discoverable when the game starts, have the client ready and
connect as soon as it appears, before advancing the title screen. Opening the
simulator window alone is insufficient: press **Connect** so it sends tracked head
poses. The menu cannot anchor without tracking.

1. Use the **left stick + A** to select offline Play, New, and a class. D2R's own
   controller UI handles class selection; the injected laser is not used for it.
2. Select the name field, click the D2R window to give it keyboard focus, and type
   the name on the Mac keyboard. Release controller keys before switching windows.
3. Return focus to the simulator (or resume the headset controller), then use
   stick + A to finish creation and enter the game. No `--flat` restart is needed.

For the local lab simulator build: **I/K/J/L** = left stick up/down/left/right;
**B** = A/confirm; **N** = B/back; **Z/X** = X/Y; **Space/V** = right/left trigger.
Keep the simulator frontmost while using these keys and release each press.
**Tab** toggles right-hand mouse aim; press it again to reset the hand aim and
return to mouse head look. The existing backtick toggle aims the left hand.
These keyboard mappings are lab additions; the stock OXRSys app may differ.
Stick + A works while the simulator has focus. Laser mouse clicks still require
D2R focus. On Quest use the physical left thumbstick and A button.

The front-end focus fix is unit-tested and cross-built. Character creation and
text entry still require live qualification on the simulator and a real Quest.

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
- The mod supports D2R 3.3.93847. Setup refuses other builds.
- Modding is your choice and the mod is not Blizzard's. Online play is never used by this mode.
