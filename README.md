# Soju

> *Wine → Whisky → Kegworks… and now a Korean round: **Soju** 🍶*

**Run Battle.net and Diablo II: Resurrected on Apple Silicon Macs — with a fully free, open-source Wine stack.**

Built from CodeWeavers' published GPL sources (Wine 11.0, CrossOver 26.3 source drop), compiled and assembled by scripts in this repo. No paid software required.

> Status (2026-08): **Working end-to-end** — Battle.net login, Agent, and D2R in-game rendering (D3DMetal), verified on an M4 Pro running macOS 26.5.

*[한국어 README](README.ko.md)*

## Why this exists

Community Wine builds (Whisky, Kegworks-era engines) stopped working with modern Battle.net and D2R. The commercial option works, but the underlying Wine engine is GPL — so we built it ourselves and documented every wall we hit. Three of those walls had never been publicly solved:

### The three keys 🔑

1. **`ROSETTA_ADVERTISE_AVX=1`** — D2R's loader requires AVX instructions. Without this env var, the game freezes forever at ~86 MB RSS with 0% CPU, before any graphics init. This is why D2R "hangs at launch" on non-CrossOver Wine builds.

2. **D3DMetal symlink layout** — Apple's Game Porting Toolkit libraries must live in `lib/external/` with `lib/wine/x86_64-unix/{d3d10,d3d11,d3d12,dxgi}.so` as **symlinks** into it. Copying the files instead breaks `@loader_path` resolution and D3DMetal dies in an assertion loop.

3. **Battle.net Agent caller-signature fix** — the Agent verifies the signature of the connecting client using a relative filename resolved against its CWD (a versioned subfolder). Seeding a copy of the signed `Battle.net.exe` into each `Battle.net.NNNNN` subfolder makes verification pass (fixes error `BLZBNTBNA00000005`).

Plus one trap: never put Apple platform binaries (`nohup`, `arch`, …) in the launch chain — macOS strips `DYLD_*` variables when exec'ing them.

## Install (one line)

```bash
curl -fsSL https://raw.githubusercontent.com/BCD1210/soju/main/install.sh | bash
```

Downloads the prebuilt engine (~350 MB), walks you through Apple's free GPTK download (the one file Apple forbids redistributing), auto-installs Battle.net with Blizzard's official installer, and drops a `Battle.net.app` in `~/Applications`. Log in and play.

## Build it yourself instead — no CrossOver required

The only thing you download outside this repo's scripts is Apple's free Game Porting Toolkit (one file, needs a free Apple ID) — Apple forbids redistributing it.

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

## Steam (including the Steam version of D2R)

D2R shipped on Steam in Feb 2026 as the *Infernal Edition*. Steam support uses a **different free engine**: the modern Steam client's CEF UI does not render on CrossOver-source builds (black window — cross-process swapchain + CEF sandbox issues), but it works on Homebrew's `wine-stable` 11 with a tiny `steamwebhelper` wrapper that forces `--disable-gpu --single-process`. That fix comes from [notpop/steam-on-m1-wine](https://github.com/notpop/steam-on-m1-wine) (MIT — wrapper source vendored in `third_party/`). Steam gets its own bottle so the two stacks never interfere:

```bash
scripts/create-steam-bottle.sh   # installs wine-stable + wrapper + official Valve installer
scripts/play.sh steam            # log in -> install & play
scripts/play.sh steam-kill       # stop the Steam bottle
```

Verified on M4 Pro / macOS 26.5: login UI renders and a real account login succeeds. Notes: switch your macOS input source to English (ABC) when typing in Steam (IME composition shows as `?` otherwise). D3D11 game rendering under this stack may additionally want the DXMT fork from the upstream project — see their `docs/building-for-games.md`.

**Prerequisites**: Apple Silicon Mac, Rosetta 2, Xcode Command Line Tools, Homebrew, your own Battle.net account/games, and a free Apple ID for the GPTK download. Nothing from Apple, Blizzard, or CodeWeavers is redistributed by this repo.

### Why is GPTK needed at all?

`libd3dshared.dylib` (inside GPTK) is not just graphics: D2R's loader requires its *non-native code region registration* to get through Rosetta 2 — without it the game freezes at launch even with AVX advertised. Graphics itself can run on pure open-source vkd3d/MoltenVK if D3DMetal is absent.

### Troubleshooting

- **"Wine Mono Installer" popup** -> step 2 was skipped; run `get-components.sh` then re-run `build-engine.sh`.
- **Game hangs forever at ~86 MB RAM, 0% CPU** -> `ROSETTA_ADVERTISE_AVX=1` or `libd3dshared` not reaching the game. Launch via `play.sh` only, and check step 4.
- **Libraries not found (gnutls/freetype errors)** -> you launched wine through `nohup`/`arch`/another Apple-signed binary, which strips `DYLD_*` vars. Launch via `play.sh`.
- **Battle.net login webview may flicker (~once a minute)** -> known cosmetic issue; it recovers automatically and login works.
- **BLZBNTBNA00000005** -> `play.sh` seeds the signed exe automatically; make sure you launch through it.

## What's in the repo

- `scripts/build-engine.sh` — full engine build recipe (freetype cross-build, gnutls wiring, GPTK layout, entitlements)
- `scripts/setup-bottle.sh` / `scripts/play.sh` — bottle creation and the verified launch environment
- `docs/DIAGNOSIS.md` — deep-dive on the CEF renderer crash and the Agent signature failure
- `docs/D2R-GAME-LAUNCH.md` — the D2R loader investigation, dead ends included
- `research/` — raw log evidence for the findings

## Licensing

- Scripts and docs in this repo: **GPL-3.0** (see LICENSE)
- The engine is built from CodeWeavers' published Wine sources (GPL/LGPL) — sources at `media.codeweavers.com/pub/crossover/source/`
- **Not included and never will be**: Apple D3DMetal/GPTK binaries (non-redistributable), CrossOver application binaries, any Blizzard files
- This project is not affiliated with CodeWeavers, Apple, or Blizzard Entertainment. Battle.net and Diablo are trademarks of Blizzard Entertainment. Use at your own risk; online play with third-party compatibility layers is at your own discretion.
