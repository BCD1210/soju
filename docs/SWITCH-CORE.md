# Experimental Switch homebrew

Soju's own Rust Hypervisor runner now runs the local 2048 homebrew v1.1.0
end-to-end on Apple Silicon macOS: a native SDL window, keyboard input,
persistent saves and normal Escape exit. The launcher selects the built Rust
runner by default. A synthetic tone NRO reaches a real CoreAudio AudioUnit.
This completes the bounded M1 implementation and automated acceptance described
below; it does not establish general Switch or retail-game compatibility.

No commercial Switch game, online service or physical gamepad has been verified.
Synthetic NSP/XCI loading and a synthetic Program-NCA boot are verified below. The runtime is experimental and is not a security sandbox;
run only files you trust. No keys, firmware, games or third-party emulator code
are included in this implementation.

## Try it

Requirements: Apple Silicon macOS, Xcode command-line tools, Cargo, Python 3 and
SDL2 headers/library. The desktop build defaults to `/opt/homebrew`; set
`SOJU_SDL_PREFIX` for another SDL2 installation.

```sh
brew install sdl2 rust
bash scripts/soju switch setup
bash scripts/soju switch play /path/to/2048.nro

# Explicit fallback, also honored by Library Play:
SOJU_SWITCH_RUNNER=python bash scripts/soju switch play /path/to/2048.nro
```

Setup builds the release `soju-switch-core` with `desktop` (which enables `hv`),
installs it in `~/.local/share/soju/switch/`, signs that copy ad-hoc with
`com.apple.security.hypervisor`, and verifies its signature. It also prepares
the existing Python/Unicorn fallback and host-font assets. Python 3.9 uses
fonttools 4.60.2; newer Python uses 4.68.0. Unicorn is pinned to 2.1.4.

`SOJU_SWITCH_RUNNER=auto` is the default: use the installed Rust binary if present,
otherwise Python. `rust` requires the binary and reports an error if absent.
`python` forces Unicorn. Runtime failures are reported; a failed Rust session
is not silently restarted in another runner. `SOJU_SWITCH_HOME` overrides the
runner/assets/save directory. This local work has not been published or pushed.

Click the window to focus it. Arrows move tiles, B (or Z) resets the board,
and Escape closes it. Other input-crate mappings include X/Z/C/V for A/B/X/Y,
WASD and IJKL for sticks, Q/E for shoulders, 1/3 for triggers, Return for Plus,
and Delete for Minus. Short key presses are latched through two presented frames (at most 500 ms)
so a press and release between guest input scans is not lost. Focus loss clears input.

GameController.framework supplies the first connected extended gamepad's
buttons, triggers and sticks. `soju-switch-input` applies positional face-button
mapping, dead zones and Npad bits; keyboard and pad buttons combine, with an
active keyboard stick taking priority. No physical pad/hotplug acceptance has
been performed. There is no multiplayer or controller-selection UI.

The native app's Setup and Library Play use the same shell launcher:

```sh
bash scripts/soju switch import /path/to/own-game.nro
bash scripts/soju switch list --json
bash scripts/soju game switch:<id-from-list>
bash scripts/soju switch remove switch:<id-from-list>
```

Imports live under `~/.battlenet-macos/switch/games/` (override with `SOJU_BASE`),
with mode 0600 and collision checks. IDs use the first 16 SHA-256 hex digits.
Removal deletes imported copies, not sources or saves. NSP/XCI metadata can be
listed through the separate filesystem crate, but retail playback is refused.

## M1 acceptance

Tested on 2026-11-03, M4 Pro, macOS 26.5 (25F71), using the user's local
2048 v1.1.0 NRO, SHA-256:
`cd7e7f343830920196590d99c82a9f1ab8a375eeaeb943fa6c671aa68250a20d`.
The NRO and its assets are not committed.

```sh
SOJU_SWITCH_TEST_NRO="$HOME/.local/share/soju/switch/demo/2048-1.1.0.nro" \
  bash scripts/test-switch-homebrew.sh
bash scripts/test-switch-audio.sh
```

The signed release homebrew test verifies a 1280×720 frame, that **each** of
left/down/right changes the board, that 2048 writes `/switch/2048/state` into
its persisted SD snapshot, and that a newly created VM/service/filesystem
restores the final framebuffer byte-for-byte from disk. Both runtime instances
exit normally. Test homes are isolated under `dist/`.

A native-window check used real macOS keyboard events: tiles moved and merged,
the score reached 4, and Escape returned process status 0. A fresh desktop
process restored that exact board and score. Escape signals the
applet exit event and allows up to five seconds for teardown; a nonresponsive
guest produces an error. Guests without an applet event can close immediately.
The missing VI DestroyStrayLayer teardown command was implemented using the
[Switchbrew description](https://switchbrew.org/wiki/VI_services#DestroyStrayLayer).

The audio test generates its own NRO with ARM64 instructions and HIPC/CMIF
messages. It connects to SM, opens audout, appends 0.5 seconds of 440 Hz stereo
S16 PCM at 48 kHz, starts playback, sleeps, stops and exits. CoreAudio's render
callback consumed all **47,920 expected nonzero samples**. No audio capture or
human listening check was performed; real-homebrew music remains unverified.

## Measured speed

The cache-maintenance path was the steady-state bottleneck. A signed release
baseline on 2026-11-03 measured 60 frames in 6.559738 s (**9.147 fps**) against
Unicorn's 18.918 fps. It generated 3,686,700 system-register exits (3,686,400
cache operations), 241 SVC exits and 13 canceled exits: **562,058 exits/s**.
`sample <pid> 3 1` captured the worker/Hypervisor path and watchdog thread churn;
the guest flushed 61,440 individual 64-byte cache lines per frame.

The runner now coalesces a recognized data-cache loop (DC, ADD by 64, CMP,
B.LO) into at most 256 KiB per trap. It validates executable loop instructions
and every readable operand page, flushes each host backing page, and leaves
the final ADD/CMP/branch to the guest to preserve flags. Unrecognized loops,
non-64-byte strides, inaccessible ranges and instruction-cache operations retain
the scalar path. No guest code is patched. Synthetic signed tests check registers,
NZCV, fallback strides and faults, alongside existing cross-vCPU code-publication
tests. The watchdog remains bounded and unchanged.

Afterward, with the runtime yielding on each completed frame, 60 headless frames
took **0.095226 s (630.08 fps)**: 1,200 system-register exits (900 cache operations),
241 SVC exits and zero canceled exits, or **15,132 exits/s**. This measures guest
frames plus conversion without PNG encoding. Unicorn in that run measured
15.456 fps; host load was not controlled. Earlier optimized samples without
immediate frame yielding measured 409–427 observed frames/s and dropped some
intermediate frames. The automated input test now allows wall-clock time for
animation/debounce, retaining three distinct input changes and exact save restore.

The desktop yields as soon as Binder queues a frame, requests SDL presentation
vsync, and uses absolute 60 Hz deadlines. It does not run extra guest frames while
waiting. A 600-frame window run took 9.976383 s from first presentation through
teardown, approximately **60 fps**. Full process `/usr/bin/time -l`: 10.55 s real,
1.56 s user, 0.73 s system (about **22% of one core**, including startup/teardown).
Its 14,639 total VM exits over the full process duration average **1,388 exits/s**.
Totals include startup: 12,008 system exits (9,000 cache), 2,607 SVC, 24 translation
maintenance, zero canceled. Conversion/copy took 648.64 ms; total IPC 669.36 ms
(includes conversion); worker round trips 583.65 ms; SVC 22.41 ms; service polling
8.94 ms; process-lock acquisition 0.788 ms. Kernel idle sleeps were zero for this
workload; pacing sleeps and presentation account for most remaining wall time.
This is one window measurement, not a guarantee under every host load.

Reproduce the window benchmark with a signed desktop release and an isolated home:

```sh
SOJU_SWITCH_HOME="$PWD/dist/perf-home" SOJU_SWITCH_BENCH_FRAMES=600 \
  /usr/bin/time -l tools/switch/core/target/release/soju-switch-core \
  --play "$HOME/.local/share/soju/switch/demo/2048-1.1.0.nro"
```

`SOJU_SWITCH_BENCH_FRAMES` requests graceful exit after the selected presentation
count and prints exit categories, lock wait, service and conversion timing.
Local raw evidence is under `~/.local/share/soju/switch/proof/performance/`.

## Architecture and boundaries

`tools/switch/core/` contains the NRO loader, bounded HLE kernel, shared
HIPC/CMIF/TIPC parser and typed service registries. `hv` enables
Hypervisor.framework; `desktop` adds SDL, GameController.framework and AudioUnit.
The imported input/audio crates are dependencies and were not modified.
The separate fs/texture crates likewise remain owned by their existing developer.

The CPU uses a stable, bounded 512 MiB host arena and permanent thread-affine
vCPUs, one host thread per guest thread. A process guard serializes guest and
host memory access. EL0 runs under 4 KiB stage-1 mappings: text RX, rodata R,
data/BSS/stack/TLS RW. Private EL1 vectors and page tables are inaccessible to
EL0. Mapping generations maintain translations; executable writes maintain
instruction caches. Stage 2 permits RWX, with protection enforced at stage 1.
Overlapping segment pages are rejected. Guest quanta are bounded and resumable.

Timer reads use a shared monotonic 19.2 MHz epoch, not cycle-accurate timing.
Cache operations use a deterministic 64-byte CTR profile. The kernel implements
the Python runner's memory, heap, thread, event, handle, wait, mutex, condition,
address-arbitration, GetInfo, debug, break and exit families. Scheduler/timing and
several Horizon responses remain simplified.

Service families cover SM, settings, applet, HID, time, FSP, VI/Binder, NV,
audout, account, power and shared fonts. Sessions and domain children preserve
kernel object identity. Reply publication is checked; staged GPU memory effects
commit before fence advancement, with reverse-order rollback on failure.
Unknown interfaces, commands and unsupported GPU submissions fail explicitly.
Inline HIPC receive-list mode 1 is not implemented. BSD registration exists,
but networking is not implemented. Accounts use one synthetic offline user.

The display is a bounded CPU framebuffer/command subset, not a general
Maxwell-to-Metal renderer. A main-thread SDL window presents RGBA and polls
input between runtime slices. Slow service work can still delay input/audio;
this is not a real-time scheduler.

Account saves retain the Python JSON snapshot format and require explicit guest
filesystem commit. Rust additionally persists virtual SD to per-NRO
`saves/<sha256>/sd-state.json` on flush or graceful shutdown. Stores use exclusive
locks, no-follow directory traversal and atomic replacement. The selected NRO,
including appended RomFS assets, is exposed read-only. Interrupted shutdown and
general save compatibility have not been established. Python SD remains in memory;
its account saves share the format, but it does not restore Rust's SD snapshot.

Shared fonts use installed macOS font fallbacks, optionally a local BMP subset
of Arial Unicode. These are not Nintendo fonts and do not guarantee all glyphs.

Audout supports 48 kHz stereo S16. Host mixing uses `soju-switch-audio` in 240-frame
blocks, clamps the mixed output, and sends it to the default AudioUnit output.
Host queues are bounded to 16 voices and 8 MiB PCM, with at most 50 ms submitted
ahead. The callback uses a single-producer/single-consumer atomic ring, allocates
nothing, and emits silence on underrun. Pause/close may leave up to 50 ms already
submitted. Guest buffer release still follows the service's timed model, not
hardware completion. Device-open failures are explicit. There is no audren
service, device-selection UI, resampling negotiation or verified real-game audio.

`tools/switch/trace_nro.py` remains the Unicorn fallback. The tiny
`tools/switch/soju-switch.c` interpreter is only a synthetic fixture runner.
Earlier Python-only checks covered TooTallNate's 2048 1.2.0 (play/save/restart)
and Snakey NX 0.4.0 (stopped on unsupported graphics). Those results do not
establish Rust compatibility for either NRO.

## Program loader skeleton (M4 preparation)

`program::Program::load(storage, extension, keys)` uses the existing filesystem
crate without modifying it. It selects one Program NCA from NSP or XCI secure
content, reads its ExeFS, and maps `rtld`, `main`, `subsdk0` through `subsdk9`
when present, and `sdk` when present. Text is RX, rodata R, data/BSS RW; compressed
NSO segments and requested hashes are handled by the filesystem crate. Page overlap,
unknown NSO flags, malformed metadata, duplicate Program NCAs, resource limits and
unsupported capabilities are explicit errors. No filenames or keys are fetched.

The skeleton accepts the AArch64/39-bit NPDM profile only, with a bounded fixed
layout inside that address space. It sets the main stack size, priority, virtual
core and affinity from NPDM, reports its address-space bounds, applies ThreadInfo
priority limits and EnableSystemCalls masks, and rejects all other capability
types (except all-ones padding). The scheduler remains serialized; additional
thread creation still supports virtual core zero only. Nonzero system resources,
other address-space profiles and newer process flags are rejected. ACID signatures
and service/FS access-control authorization are not verified; this is not a secure
loader or general retail compatibility claim.

`Runtime::from_program` starts at `rtld` with the process main-thread handle.
`Hub::new_program` mounts the Program RomFS as read-only FSP current-process
storage (command 200; storage Read/GetSize). It uses a separate program-ID save
namespace. RomFS is bounded to 256 MiB, with reads limited to 8 MiB and validated
against guest memory and IPC TLS. Retail playback is not exposed by the launcher.

Tests construct all NSOs, NPDM, RomFS and NSP/XCI bytes themselves, using
`nca::fixture` and made-up keys entirely in memory. The signed fixture executes
its own `rtld` branch into its own `main`, which reaches ExitProcess successfully;
removing the SVC capability produces an explicit unsupported-SVC stop. Portable
tests verify module order/permissions, both containers, main thread metadata,
RomFS bytes through the FSP storage object, out-of-range reads, rejected writes,
missing keys and invalid metadata. The fixture does not implement dynamic linking:
relocation/import handling remains the guest rtld's responsibility and has not
been exercised with an SDK.

A typical real program may stop at an unsupported capability before executing.
If its metadata is accepted, expected early calls include QueryMemory/GetInfo,
SetHeapSize, thread/synchronization SVCs, ConnectToNamedPort and SendSyncRequest,
followed by SM, FSP, applet, HID, NV and audio interfaces. This is an expectation
from the available interfaces, not a trace of a commercial title. Audren, general
retail services, patch/BKTR mounting, full capabilities and real rtld behavior are
not verified. References: [NPDM](https://switchbrew.org/wiki/NPDM) and
[NSO0](https://switchbrew.org/wiki/NSO0).

## GPU merge status (M2 incomplete)

`feat/switch-shader` was merged at `59db612`, including the persistent Metal
worker, deferred readback, fences and eight-target support. Neither imported
crate was edited. Its nine existing GPU tests pass, including four native Metal
render/resource tests. These test that crate's synthetic method streams only.
The runner's NV GPFIFO path still uses its existing CPU command subset; it is
**not connected to this Metal backend**. Snakey NX was not downloaded or run in
this session, and there are no Snakey screenshots or frame-change acceptance
results. The official release URL is still needed; the supplied checksum alone
does not locate an authorized download. No general Switch web search was used.

## Validation commands

```sh
cargo test --offline --manifest-path tools/switch/core/Cargo.toml --features hv
cargo test --offline --manifest-path tools/switch/core/Cargo.toml --features desktop
cargo clippy --offline --manifest-path tools/switch/core/Cargo.toml --features desktop --all-targets -- -D warnings
SOJU_SWITCH_TEST_NRO="$HOME/.local/share/soju/switch/demo/2048-1.1.0.nro" bash scripts/test-switch-hv.sh
SOJU_SWITCH_TEST_NRO="$HOME/.local/share/soju/switch/demo/2048-1.1.0.nro" bash scripts/test-switch-homebrew.sh
bash scripts/test-switch-audio.sh
SOJU_SWITCH_TEST_NRO="$HOME/.local/share/soju/switch/demo/2048-1.1.0.nro" \
  "$HOME/.local/share/soju/switch/venv/bin/python" -m unittest discover -s tests -p 'test_switch*.py'
bash scripts/build-app.sh
```

Final validation: 80 pure Rust tests with `hv`, 82 with `desktop`, 20 signed
Hypervisor tests, one signed real-NRO acceptance test, one signed AudioUnit tone
test, and 97 Python Switch tests (95 passed, two optional NRO cases skipped).
Clippy passed with warnings denied. The app was built and signature-verified,
not installed or published. Fresh setup and both launcher selections were tested
in `dist/`; the user's installed runner was not replaced.

Follow-up validation: 86 portable desktop Rust tests (74 library, nine kernel,
three loader), 22 signed Hypervisor tests, 97 Python tests (95 passed, two optional
cases skipped), Clippy with warnings denied and the app build passed. The signed
2048 input/save/restart acceptance passed after the performance changes. GPU crate:
nine tests passed, including four Metal tests. No changes were pushed.
