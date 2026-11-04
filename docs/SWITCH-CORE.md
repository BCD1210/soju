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

## Rust core: M0

`tools/switch/core/` is a dependency-free Cargo workspace with a library and
`soju-switch-core` validation-only binary. `nro.rs` ports the existing Python
loader's bounded NRO header/segment checks and initial homebrew ABI mappings
(image/BSS, stack, TLS, loader config, exception context and return stub).
Appended assets are excluded from mapped image memory. The ABI is the existing
Soju diagnostic profile, not a complete Horizon process implementation.

```sh
cargo test --offline --manifest-path tools/switch/core/Cargo.toml
```

Four synthetic unit tests cover image loading, malformed/truncated inputs,
segment bounds and ABI bytes/zeroed memory. CI runs only pure Rust tests with
no default features. The launcher still uses the Python runner.

## Apple Hypervisor CPU: bring-up

The optional `hv` feature uses Apple's installed Hypervisor.framework SDK,
without external crates. It creates one VM, a 16 KiB aligned host arena and
thread-affine vCPUs. AArch64 programs run at EL0 with 4 KiB stage-1
translation enabled. Synchronous exceptions enter the EL1 vector; its HVC
returns to the host, which reads ESR_EL1 and the preserved guest registers.
Handled timer reads resume through ERET. CNTKCTL_EL1 disables direct EL0 timer
access: CNTFRQ returns 19,200,000 Hz and CNTVCT is host monotonic elapsed time
scaled to that frequency, starting at VM creation. This is not cycle-accurate.

On this Apple Silicon Mac, the following signed synthetic tests passed:

```sh
cargo test --offline --manifest-path tools/switch/core/Cargo.toml --features hv
bash scripts/test-switch-hv.sh
```

The first command runs seventeen pure tests, leaving fifteen hardware integration
tests ignored. The script builds just the integration test executable, signs
it ad-hoc with `com.apple.security.hypervisor`, verifies its signature and runs
all fifteen tests serially (the local NRO cases require the environment variable
below). It does not install or launch Soju, download anything,
or change services. CI does not run hardware tests.

Observed results:

- `mov x0,#7; svc #0x26` at EL0 produces SVC 0x26 and x0=7 at the host;
  resuming executes a second SVC with x0=9.
- Three timer accesses trap: CNTFRQ returns 19,200,000 and successive CNTVCT
  reads are monotonic. A pure test verifies nanoseconds-to-ticks scaling.
- A looping guest is canceled by the two-second host watchdog. An undefined
  instruction stops with an unsupported guest exception.

### Guest-memory milestone (step 1 of the Rust runner)

`Nro::protected_homebrew` connects the loader to `Cpu::from_homebrew`.
The three-level stage-1 tables use a 39-bit virtual address space and 4 KiB
pages backed by a compact host arena. Text is RX, rodata R, and data/BSS,
stack, TLS and loader config RW. User mappings are privileged-execute-never;
the EL1 HVC/ERET bridge is inaccessible to EL0 and table pages have no user
mapping. Stage 2 permits RWX; stage 1 enforces guest permissions. NROs with
segments sharing a page are rejected instead of merging incompatible permissions.
The homebrew config retains the Python runner's main-thread handle, argv,
applet type and end marker.

Signed synthetic tests execute from NRO text, read config/TLS, write and read
zero-initialized BSS, and verify actual permission faults for text/rodata
writes and execution of rodata, data, BSS, TLS and private EL1/table addresses.
The existing SVC-resume, virtual-timer and watchdog tests still pass.

```sh
SOJU_SWITCH_TEST_NRO="$HOME/.local/share/soju/switch/demo/2048-1.1.0.nro" \
  bash scripts/test-switch-hv.sh
```

With the local 2048 1.1.0 NRO this reaches its first SVC, `0x2`
(`SetMemoryPermission`), at PC `0x100006fcc`. This is a boot smoke test,
**not** a frame-change or gameplay test. No instructions/second comparison
has been measured. IPC service implementation, window/input and save persistence
are still pending; M1 is not complete. The binary remains validation-only and the launcher still
uses the Python runner. No retail support has been established.

### Kernel milestone (step 2)

The process now owns a stable, bounded 512 MiB arena shared by permanent
thread-affine vCPUs. A process guard serializes guest execution and host memory
access. Live mappings support 4 KiB protection, aliases, unmapping and physical
page reuse. Mapping generations drive EL1 translation maintenance before guest
re-entry; executable host writes also invalidate cached code. The runtime uses
one host thread per guest thread and bounded resumable quanta.

The bounded HLE kernel implements the Python runner's non-IPC SVC families:
heap and memory operations, threads, events, shared/transfer memory, handles,
mutexes, condition waits, timing, named ports, GetInfo, Break and exit. It also
implements address waits/signals, debug output and synchronization cancellation.
IPC requests cross an explicit handler boundary; missing services stop with
`IpcRequired` rather than returning success. The local 2048 1.1.0 NRO now passes
kernel initialization and reaches that boundary for the service manager.

Nine deterministic kernel scenarios cover memory and handle lifetimes,
blocking/wakeup/timeout behavior, condition-mutex reacquisition, address
arbitration and diagnostics. Fifteen signed hardware tests additionally cover
distinct per-vCPU TLS/SIMD state, shared memory, warmed translation revocation,
instruction replacement, syscall replies, resumable execution, multithreaded
synchronization and a shared 19.2 MHz epoch for CNTVCT, CNTPCT and GetSystemTick.
This does not establish complete Horizon kernel compatibility or gameplay.

Validation on 2026-11-03: the full Python suite ran 320 tests with 21 skips
and only the known `test_only_attributed_commits_fail` failure. The Switch
venv suite ran 92 tests with three optional tests skipped and no failures.
`bash scripts/build-app.sh` built and signature-verified the native app; it
was not installed or launched. All new game fixtures were synthetic.

### IPC milestone (step 3)

The Rust core now has a shared bounded HIPC/CMIF/TIPC codec and typed command
registries. Services cover the Python runner's implemented settings, applet,
HID, time, filesystem, VI/Binder, NV, audio, account, power and font interfaces.
Both appletAE and appletOE roots are recognized. Unknown interfaces and commands
fail explicitly; this is a limited compatibility profile, not full Horizon IPC.
Inline HIPC receive-list mode 1 is not implemented.

Session aliases share stable kernel object identities. Domain children retain
those identities until their owning domain closes. Reply handles are published
with checked descriptor and TLS writes. GPU submissions stage their memory effects
and only advance fences after the reply transaction commits; failed writes roll
back in reverse order. The CPU-only GPU command subset remains bounded and does
not implement a general Maxwell renderer.

Filesystem snapshots retain the Python account-save JSON format, with strict
validation, no-follow path traversal, an exclusive lock, and atomic replacement.
The Rust virtual SD remains in memory during execution and additionally persists
a separate per-NRO `sd-state.json` at explicit flush or graceful shutdown. This
allows homebrew which saves on SD to restore after restart. Account saves still
require their own explicit filesystem commit. The selected NRO is exposed
read-only, including appended assets, for the guest's RomFS reader.

The audio service has a bounded timed silent sink and a host PCM queue. Audible
game audio has not been established. Shared fonts use installed host fallbacks.

The local 2048 1.1.0 NRO produced a nonuniform 1280×720 framebuffer through
this Rust IPC path in 0.555 seconds in one debug hardware run. This is a
first-frame smoke result, not an instructions/second measurement or interactive
acceptance. libnx's cache maintenance uses a deterministic 64-byte CTR profile
and checked cache operations; the host's cache geometry is not exposed through
CTR. Input, restart restoration and the window path require separate acceptance.
