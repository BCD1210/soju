# Soju

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

## Quick start

```bash
# 1. Build the engine from GPL sources (takes a while; downloads ~150 MB source)
scripts/build-engine.sh

# 2. Create a bottle (copies an existing Battle.net install, registry included)
DEST=~/.battlenet-macos/bottle scripts/setup-bottle.sh

# 3. Play
scripts/play.sh battlenet   # launcher → log in → hit Play
scripts/play.sh d2r         # direct game launch
scripts/play.sh kill        # stop everything
```

**Prerequisites**: Apple Silicon Mac, Rosetta 2, Xcode Command Line Tools, Homebrew, and your own copies of the game/Battle.net (this repo ships no Blizzard files). Initial components (the bottle, D3DMetal, and a few x86_64 dylibs) are harvested **once** from a local CrossOver install — the free trial is sufficient. Nothing from Apple, Blizzard, or CodeWeavers is redistributed by this repo.

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
