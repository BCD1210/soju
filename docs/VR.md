# Diablo II: Resurrected in VR (offline, experimental)

Soju can set up [D2R VR Unleashed](https://github.com/bearstonem/d2r-pcvr) (D2R-PCVR 0.3.31, MPL-2.0), a
first-person VR mod for **offline characters**, and stream it to a headset from your Mac.

```bash
soju vr setup      # once (about 2 minutes, ~110 MB of downloads)
soju vr play       # every time
soju vr status
soju vr setup --refresh   # after a D2R update or when the 30-day check appears
soju vr remove --yes
```

In the app: right-click Diablo II: Resurrected in the library, then **Set Up VR (offline)** and **Play in VR**.

## What it does

- Makes an APFS copy of your Battle.net bottle in `~/.battlenet-macos/d2r-vr/bottle`. The copy shares disk space with
  the original until files change, and **your Battle.net bottle is never modified**. Normal Play still starts the stock
  game.
- Downloads, each pinned by size and SHA-256 in `resources/d2r-vr.json`: the mod's release ZIP (GitHub), D2RLoader
  1.3.1-beta (d2rloader.net), ReShade 6.8.0 (reshade.me; only `ReShade64.dll` is extracted, the setup never runs),
  Windows embeddable Python 3.12 (python.org) for the mod's own installer, and Soju's VR runtime.
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

## Notes

- Use **offline characters** only. Characters you make in VR live in the copy; `--refresh` keeps them.
- D2R requires an online sign-in within the last 30 days even for offline play. If it says so, play D2R online once
  from Battle.net, then run `soju vr setup --refresh`.
- The mod supports D2R 3.3.93847. Setup refuses other builds.
- Modding is your choice and the mod is not Blizzard's. Online play is never used by this mode.
