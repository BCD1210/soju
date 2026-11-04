# Experimental Switch homebrew

Soju's own Rust Hypervisor runner now runs the local 2048 homebrew v1.1.0
end-to-end on Apple Silicon macOS: a native SDL window, keyboard input,
persistent saves and normal Escape exit. The launcher selects the built Rust
runner by default. A synthetic tone NRO reaches a real CoreAudio AudioUnit.
This completes the bounded M1 implementation and automated acceptance described
below; it does not establish general Switch or retail-game compatibility.

No commercial Switch game, NSP/XCI execution, online service or physical gamepad
has been verified. The runtime is experimental and is not a security sandbox;
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

`scripts/test-switch-homebrew.sh` measures first converted frame and the next
60 headless frames, then runs the same frame count on Unicorn. Two runs:

| Runner | First frame | Next 60 frames | Frames/s |
| --- | --- | --- | --- |
| Rust release, run 1 | 0.312 s | 7.876 s | 7.62 |
| Unicorn 2.1.4, run 1 | 0.456 s | 3.628 s | 16.54 |
| Rust release, run 2 | 0.186 s | 6.454 s | 9.30 |
| Unicorn 2.1.4, run 2 | 0.347 s | 2.141 s | 28.03 |

Both paths include service handling and RGBA conversion on every measured frame,
without PNG encoding or a host window. Python uses the installed native frame
converter. Each starts with an empty save home and no input during measurement.
Rust is **slower in steady state** here: 0.33–0.46× Unicorn's throughput, despite
a faster first frame. Host load was not controlled; the spread is substantial.
These are observations, not a performance guarantee. Hypervisor guest instruction
counts are not available, so this is a frames/second comparison.

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
