# Packaged Unreal Engine builds (Windows)

`soju ue` starts a Windows build of an Unreal Engine 4 or 5 game that you have as a folder: your own project's
package (File > Package Project > Windows), or a build a developer sent you to test. It runs on Soju's CX engine with
Apple's D3DMetal, in the same environment as Xbox PC games (`scripts/xbox-wine.sh`), and each project gets a bottle
of its own in `~/.battlenet-macos/ue-bottles/<Project>`. It needs the engine and Apple GPTK (`soju install`, then
`soju gptk`). The game folder is only read.

```sh
soju ue ~/Games/MyProject/Windows                       # windowed at 1280x720
soju ue ~/Games/MyProject/Windows -ResX=1920 -ResY=1080 -windowed   # your arguments replace the default ones
```

What it does:

- Starts `<Project>/Binaries/Win64/<Project>-Win64-Shipping.exe` directly. The `<Project>.exe` at the top of the
  package only starts that file, and starting the real one is more reliable under Wine. A package without a Shipping
  exe (Development or DebugGame) is started through the exe at the top.
- On a new bottle, installs Microsoft's VC++ 2015-2022 x64 runtime, which Unreal builds link against. It is the same
  installer the Xbox prefix uses: downloaded from Microsoft, checked by SHA-256, never redistributed.
- Turns off Unreal's "known issues with graphics driver" window, as for the other Unreal games (`ue_driver_warning.py`).
- Ctrl+C in the Terminal, or closing the Terminal window, stops everything in the bottle.

## Things to know

- **The first start is slow.** D3DMetal converts the game's shaders for Metal. On an M1 the first start took about
  6.5 minutes before the menu appeared. Leave it running.
- **Pick the graphics preset in the game.** The build tested started on the High preset and ignored a
  `GameUserSettings.ini` written into the bottle beforehand. Choose a preset in the game's own settings; it is kept
  for the next start. `soju fps on` shows the frame rate.
- **Memory.** On an 8 GB Mac the game used about 3.4 GB. With a browser open alongside, the first start swapped and
  stalled for a while. Close large apps first.
- **Logs.** Wine's output goes to `~/.battlenet-macos/logs/ue-<Project>.log` (replaced on every start). Unreal's own
  log and settings are in the bottle, under
  `drive_c/users/crossover/AppData/Local/<Project>/Saved/Logs/` and `.../Saved/Config/Windows/GameUserSettings.ini`.
  Remove account names and personal paths before posting them.
- Not covered: games with kernel anti-cheat; the DLSS-through-MetalFX bridge that Soju's launchers set up
  (`gptk-dlss.sh`) is not turned on here.

## What was checked (2026-11-25, MacBook Pro M1 8 GB, macOS 26.6, engine-v1.10, GPTK 3.0)

| Check | Result |
|---|---|
| A UE5 Direct3D 12 Shipping build, `-Win64-Shipping.exe` started directly in this environment, VC++ runtime in the bottle | menu and gameplay |
| First start | about 6.5 minutes of shader conversion |
| Lowest preset, 1280x720 windowed | 100-110 FPS |
| Low preset, 1280x720 windowed | 50-60 FPS |

The game was played with a launcher that sets the same environment and starts the same file; `soju ue` itself is
covered by `tests/test_ue_wine.py`. UE4 packages (`WindowsNoEditor/`) have the same layout but were not tried.
