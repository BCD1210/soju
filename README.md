# Soju

> *Wine → Whisky → Kegworks… and now a Korean round: **Soju** 🍶*

**The launchers actually log in.** Battle.net, Steam, the Epic Games Launcher and GOG GALAXY on Apple Silicon. No black login window, no "Signing in…" that never ends, no CrossOver license. A fully free, open-source Wine stack.

If you got here from a Whisky/Kegworks thread titled *"Battle.net crashes on login"*, *"Steam won't open"* or *"launcher shows a blank window"*: those are the exact walls this repo documents and fixes (CEF renderer `int3` on `PAGE_WRITECOPY`, CEF GPU process dying on init, Steam's webhelper). See [docs/DIAGNOSIS.md](docs/DIAGNOSIS.md).

Built from CodeWeavers' published GPL sources (Wine 11.0, CrossOver 26.3 source drop), compiled and assembled by scripts in this repo. No paid software required.

> Status (2026-08): **Working end-to-end**: Battle.net login, Agent, and D2R in-game rendering (D3DMetal); Epic Games Launcher login and game installs (tray icon in the menu bar); GOG GALAXY login; Steam client login + D3D11 games. Verified on an M4 Pro running macOS 26.5.

*[한국어 README](README.ko.md)*

## Why this exists

Community Wine builds (Whisky, Kegworks-era engines) stopped working with modern Battle.net and D2R. The commercial option works, but the underlying Wine engine is GPL, so we built it ourselves and documented every wall we hit. Three of those walls had never been publicly solved:

### The three keys

1. **`ROSETTA_ADVERTISE_AVX=1`**: D2R's loader requires AVX instructions. Without this env var, the game freezes forever at ~86 MB RSS with 0% CPU, before any graphics init. This is why D2R "hangs at launch" on non-CrossOver Wine builds.

2. **D3DMetal symlink layout**: Apple's Game Porting Toolkit libraries must live in `lib/external/` with `lib/wine/x86_64-unix/{d3d10,d3d11,d3d12,dxgi}.so` as **symlinks** into it. Copying the files instead breaks `@loader_path` resolution and D3DMetal dies in an assertion loop.

3. **Battle.net Agent caller-signature fix**: the Agent verifies the signature of the connecting client using a relative filename resolved against its CWD (a versioned subfolder). Seeding a copy of the signed `Battle.net.exe` into each `Battle.net.NNNNN` subfolder makes verification pass (fixes error `BLZBNTBNA00000005`).

Plus one trap: never put Apple platform binaries (`nohup`, `arch`, …) in the launch chain. macOS strips `DYLD_*` variables when exec'ing them.

**Guides:** [D2R on Apple Silicon](https://bcd1210.github.io/soju/guides/diablo-2-resurrected-apple-silicon.html) · [Windows Steam client](https://bcd1210.github.io/soju/guides/steam-windows-client-apple-silicon.html) · [Whisky alternatives](https://bcd1210.github.io/soju/guides/whisky-alternative.html)

## Install (one line)

```bash
curl -fsSL https://raw.githubusercontent.com/BCD1210/soju/main/install.sh | bash
```

Or via Homebrew:

```bash
brew install BCD1210/soju/soju
soju install     # then: soju battlenet / soju d2r / soju epic / soju steam
```

Downloads the prebuilt engine (~350 MB), walks you through Apple's free GPTK download (the one file Apple forbids redistributing), auto-installs Battle.net with Blizzard's official installer, and drops a `Battle.net.app` in `~/Applications`. Log in and play.

## Build it yourself instead: no CrossOver required

The only thing you download outside this repo's scripts is Apple's free Game Porting Toolkit (one file, needs a free Apple ID). Apple forbids redistributing it.

```bash
# 1. Get the scripts
git clone https://github.com/BCD1210/soju.git && cd soju

# 2. Fetch runtime components (x86_64 dylibs + wine-mono, all from free GPL releases)
scripts/get-components.sh

# 3. Build the engine from GPL sources (30-60 min)
scripts/build-engine.sh

# 4. Install Apple GPTK payload (download the GPTK dmg from
#    https://developer.apple.com/games/game-porting-toolkit/ , mount it, then:)
scripts/get-gptk.sh
#    (if you happen to have CrossOver installed, the script auto-extracts from it instead)

# 5. Create a bottle + install Battle.net (official Blizzard installer, fully automatic)
scripts/create-bottle.sh

# 6. Play
scripts/play.sh battlenet   # launcher -> log in -> install & play your game
scripts/play.sh d2r         # direct game launch (offline)
scripts/play.sh kill        # stop everything
```

Already have a CrossOver bottle with games installed? `scripts/setup-bottle.sh` clones it (28 GB game re-download avoided).

## Epic Games Launcher

Same engine, own bottle, no extra tricks: Epic's CEF keeps its GPU process alive on this build, so the launcher runs with its stock command line. Verified 2026-08-29: unattended install from the official MSI (~30 s), launcher UI, login.

```bash
scripts/create-epic-bottle.sh    # official Epic MSI, unattended
scripts/play.sh epic             # log in -> install & play
scripts/play.sh epic-kill        # stop the Epic bottle (closing the window keeps the launcher running; right-click the Epic icon in the macOS menu bar to reopen or Exit, same as the Windows tray; a plain left click does nothing, as Epic only reacts to the menu)
```

Verified 2026-08-30: 71 GB install of Hogwarts Legacy (UE4, D3D12 through D3DMetal) from the launcher, then in-game. Two things to know: switch the macOS input source to English (ABC) before playing, a Korean/Japanese/Chinese IME swallows key presses in Wine games; and if a game starts full screen, set windowed mode in its own display settings (Soju does not force it). One known wine 11.0 issue: after switching between several Wine windows (a launcher, a game, another bottle) a game can stop receiving keyboard input until it is restarted; upstream fixed this in wine 11.11.

### GOG GALAXY

```bash
scripts/create-gog-bottle.sh      # separate bottle, official web installer (silent)
scripts/play.sh gog               # log in, install and play
scripts/play.sh gog-kill          # stop the GOG bottle (closing the window parks GOG in the menu bar tray)
```

Verified 2026-08-30: install, login, library. GOG GALAXY 2.x is Qt6 + QtWebEngine, not CEF, and on this engine its window stays black because Qt's D3D11 compositing asks D3DMetal's DXGI for `IDXGIResource`, which it does not implement. The fix is to run Chromium on the CPU (`--disable-gpu`), but GOG overwrites `QTWEBENGINE_CHROMIUM_FLAGS` itself and ignores its own command line, and it checksums its executable, so nothing outside the engine can inject the switch. The engine therefore carries a small hook (`patches/chromium-flags-append.patch`): whenever a program sets `QTWEBENGINE_CHROMIUM_FLAGS`, the contents of `SOJU_CHROMIUM_FLAGS` are appended. `play.sh gog` sets it. Details in [docs/DIAGNOSIS.md](docs/DIAGNOSIS.md).

Games have not been broadly tested yet; anything that needs kernel anti-cheat (EAC/BattlEye) will not run under Wine.

## Steam (including the Steam version of D2R)

D2R shipped on Steam in Feb 2026 as the *Infernal Edition*. Steam support uses a **different free engine**: the modern Steam client's CEF UI does not render on CrossOver-source builds (black window, cross-process swapchain + CEF sandbox issues), but it works on Homebrew's `wine-stable` 11 with a tiny `steamwebhelper` wrapper that forces `--disable-gpu --single-process`. That fix comes from [notpop/steam-on-m1-wine](https://github.com/notpop/steam-on-m1-wine) (MIT, wrapper source vendored in `third_party/`). Steam gets its own bottle so the two stacks never interfere:

```bash
scripts/create-steam-bottle.sh   # installs wine-stable + wrapper + official Valve installer
scripts/play.sh steam            # log in -> install & play
scripts/play.sh steam-kill       # stop the Steam bottle (closing the window keeps Steam running; click the Dock icon to bring it back, Steam > Exit quits)
```

Verified on M4 Pro / macOS 26.5: login, library, and an actual D3D11 (Unity) game rendering in-game via a DXMT fork. See `docs/STEAM-GAMES.md` for the full wiring (`scripts/setup-steam-games.sh`). Notes: switch your macOS input source to English (ABC) when typing in Steam (IME composition shows as `?` otherwise).

**Prerequisites**: Apple Silicon Mac, Rosetta 2, Xcode Command Line Tools, Homebrew, your own Battle.net account/games, and a free Apple ID for the GPTK download. Nothing from Apple, Blizzard, or CodeWeavers is redistributed by this repo.

### Why is GPTK needed at all?

`libd3dshared.dylib` (inside GPTK) is not just graphics: D2R's loader requires its *non-native code region registration* to get through Rosetta 2. Without it the game freezes at launch even with AVX advertised. Graphics itself can run on pure open-source vkd3d/MoltenVK if D3DMetal is absent.

### Troubleshooting

- **"Wine Mono Installer" popup** -> step 2 was skipped; run `get-components.sh` then re-run `build-engine.sh`.
- **Game hangs forever at ~86 MB RAM, 0% CPU** -> `ROSETTA_ADVERTISE_AVX=1` or `libd3dshared` not reaching the game. Launch via `play.sh` only, and check step 4.
- **Libraries not found (gnutls/freetype errors)** -> you launched wine through `nohup`/`arch`/another Apple-signed binary, which strips `DYLD_*` vars. Launch via `play.sh`.
- **Battle.net login webview may flicker (~once a minute)** -> known cosmetic issue; it recovers automatically and login works.
- **BLZBNTBNA00000005** -> `play.sh` seeds the signed exe automatically; make sure you launch through it.

### Leftover Wine processes

Every bottle start also spawns idle Windows service processes (`services.exe`, `winedevice.exe`, `plugplay.exe`, `rpcss.exe`, `explorer.exe /desktop`, ~100 MB per set). If a bottle's `wineserver` is killed abruptly (a crash, an aborted run), those services never notice and linger. `scripts/soju-sweep.sh` removes them, and `play.sh kill` / `epic-kill` / `steam-kill` and the reaper call it automatically. It only runs when no `wineserver` is running at all, so a live game or launcher is never touched.

## What's in the repo

- `scripts/build-engine.sh`: full engine build recipe (freetype cross-build, gnutls wiring, GPTK layout, entitlements)
- `scripts/setup-bottle.sh` / `scripts/play.sh`: bottle creation and the verified launch environment
- `docs/DIAGNOSIS.md`: deep-dive on the CEF renderer crash and the Agent signature failure
- `docs/D2R-GAME-LAUNCH.md`: the D2R loader investigation, dead ends included
- `research/`: raw log evidence for the findings

## Licensing

- Scripts and docs in this repo: **GPL-3.0** (see LICENSE)
- The engine is built from CodeWeavers' published Wine sources (GPL/LGPL). Sources:  `media.codeweavers.com/pub/crossover/source/`
- **Not included and never will be**: Apple D3DMetal/GPTK binaries (non-redistributable), CrossOver application binaries, any Blizzard files
- This project is not affiliated with CodeWeavers, Apple, or Blizzard Entertainment. Battle.net and Diablo are trademarks of Blizzard Entertainment. Use at your own risk; online play with third-party compatibility layers is at your own discretion.
