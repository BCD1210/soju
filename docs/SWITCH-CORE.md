# Experimental Switch homebrew

This is an early Soju-owned Switch compatibility experiment, not a general
Switch emulator. It runs the published MIT-licensed
[2048 Switch homebrew v1.1.0](https://github.com/BernardoGiordano/2048/releases/tag/1.1.0)
on an Apple Silicon Mac. No commercial Switch game, NSP/XCI, online service,
game controller, or general homebrew compatibility has been established.
The virtual SD directory and files are **in memory only**. Experimental account
saves persist when the game commits them, as described below.
The selected NRO is exposed as a read-only virtual file for embedded assets.
The experimental runtime is not a security sandbox; run only files you trust.

## Try it

On Apple Silicon macOS, with Python 3 and SDL2 installed:

```sh
brew install sdl2  # if SDL2 is not already available
bash scripts/soju switch setup
bash scripts/soju switch play /path/to/2048.nro
```

Use the arrow keys to move tiles, B to start a new board, and Escape to close.
Click the window first if it is behind another app. The local app build includes a Nintendo Switch card in Platforms and imported
games in Library. Use Setup and Add game file, or the commands below.
This integration has not been published as a release.

```sh
bash scripts/soju switch import /path/to/own-game.nro
bash scripts/soju switch list --json
bash scripts/soju game switch:<id-from-list>
bash scripts/soju switch remove switch:<id-from-list>
```

Imports keep the filename in `~/.battlenet-macos/switch/games/` with mode
0600 and refuse filename collisions. IDs use the first 16 hex digits of the
whole file SHA-256. NRO titles use an embedded NACP name when available.
NSP/XCI files can be imported and listed, but cannot be played. Removing an
ID removes its imported copies, never the source or saves. `SOJU_BASE` can
override the library root for synthetic tests.

## Validation

On 2026-11-03, the upstream 2048 v1.1.0 NRO (SHA-256
`cd7e7f343830920196590d99c82a9f1ab8a375eeaeb943fa6c671aa68250a20d`)
was run on an Apple Silicon Mac. A keyboard move changed the board and combined
tiles; the on-screen score rose to 4. This verifies this homebrew game, not
compatibility with retail titles. Neither the NRO nor its assets are committed.

To run the optional real-game test along with the synthetic tests:

```sh
SOJU_SWITCH_TEST_NRO=/path/to/2048.nro \
  "$HOME/.local/share/soju/switch/venv/bin/python" -m unittest discover -s tests -p 'test_switch*.py' -v
```

The automated test compares frames before and after three virtual controller
inputs. The manual check additionally used real macOS keyboard events in the
SDL window: arrow keys moved and merged tiles, B reset the score and board,
and Escape exited successfully. A later check covered a looping guest that
never presents a frame and verified that its window can still close.

Two other publicly distributed homebrew NROs were also tried on 2026-11-03:

- Snakey NX 0.4.0 (SHA-256 `ecacb34c33afbbfa3ab457d33a7ce157e6ac65168122a895fba4eb5aebfdc469`): initializes its legacy services, audio, GPU channel and address space. Its 847-method initialization submission completes on the CPU; its next submission reaches staged tiled color clear and stops on unsupported graphics commands. No frame or play has been verified.
- TooTallNate's 2048 1.2.0 (SHA-256 `51f6588000d899c58faadc4443f9550f4e4bcc93719fabb3dc0537ec11037ff5`): displayed a game board; real keyboard arrows moved tiles, produced a 4 from two 2 tiles, and increased the score from 0 to 4. A later session reached score 16, committed the save and exited normally; the exact board and score were restored on restart. Full-game completion was not tested.

These two 2048 homebrew games are the only games verified to accept play input. This is
early research code: timing, memory permissions, IPC,
filesystem, thread synchronization and graphics handling are incomplete and game-specific. Save
persistence for other games, game audio and other controllers are not verified.

## Architecture

`tools/switch/soju-switch.c` is a tiny bounded interpreter for a synthetic
NRO fixture. `tools/switch/trace_nro.py` is a separate experimental
NRO runtime with experimental cooperative thread scheduling using the third-party Unicorn ARM64 CPU engine,
Soju's own limited Horizon service and framebuffer implementation, and SDL2
for the host window. It is not an nxnative wrapper. The setup command installs
Unicorn into an isolated Soju virtual environment and builds an optional native
frame converter when a compiler is present; SDL2 is a separate runtime dependency. See the source projects' licenses before redistributing dependencies.

The experimental virtual GPU reports a 4 KiB ZCull context allocation contract;
this is a Soju profile, not a measured hardware value. A bounded CPU-only command
subset can commit memory writes and signal completion; unsupported rendering
submissions stop without being committed. No Maxwell-to-Metal renderer exists yet.

The legacy PCM audio path can queue 48 kHz stereo S16 data to SDL when available;
it has only been exercised with a dummy SDL device and has not been verified
with an audible game. Headless runs use a timed silent sink.

Virtual SD card writes are temporary. Experimental account saves persist when the
game commits them, under a namespace derived from the NRO's SHA-256 hash. One
2048 homebrew game has been verified to save, exit normally, and restore its
board and score after restart; other games and interrupted shutdowns are not yet verified. Player selection uses
a single synthetic offline user and does not access or represent a Nintendo Account.
BSD registration is emulated but sockets are not implemented.
The shared-font service uses installed macOS fonts as fallbacks. Setup optionally
creates a local Basic Multilingual Plane subset of Arial Unicode for CJK coverage;
if unavailable, it falls back to Latin Arial. Neither fallback is a Nintendo font,
provides Nintendo-specific icons, or guarantees complete CJK coverage. Read-only access to the selected NRO is
implemented experimentally and has not yet been validated by a game that uses
its embedded RomFS. The simulated system clock and several
service responses are simplified. Unsupported calls stop with a diagnostic.

NRO layout references:
[switch-tools](https://github.com/switchbrew/switch-tools/blob/master/src/elf2nro.c),
[homebrew loader](https://github.com/switchbrew/nx-hbloader/blob/master/source/main.c),
[Homebrew ABI](https://switchbrew.org/wiki/Homebrew_ABI), and
[libnx](https://github.com/switchbrew/libnx).
