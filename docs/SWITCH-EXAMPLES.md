# Official devkitPro example corpus

Follow-up verification on the Mac, after merging `feat/switch-shader` at
`2b31d37` and `feat/switch-fs` at `a956946`. Follow-up commits are local only;
nothing was pushed in this pass. The earlier checkpoint `04176c2` was already pushed.

**First-frame result: Unicorn 2 / 59; Rust 33 / 59 (previously 32).**
Rust exits without a frame for one example; 25 stop before a frame. These are
execution milestones, not 33 fully supported applications. **M2 is incomplete.**
An opt-in Metal submission boundary now executes a synthetic native clear, but
no deko3d/OpenGL example has an accepted GPU-rendered frame. The deko selector
is a CPU console and is not GPU evidence.

The original 59-example denominator is unchanged. Three newly built supplemental
programs are reported separately below, including verified host-font rendering.

## Reproduce

```sh
bash scripts/test-switch-examples.sh
SOJU_SWITCH_FRAMES=60 SOJU_SWITCH_TIMEOUT=30 \
  SOJU_SWITCH_PROOF="$HOME/.local/share/soju/switch/proof/rust-examples-60" \
  bash scripts/test-switch-examples.sh
```

The script builds `hv` offline, copies the executable into an isolated temporary
directory, signs that copy, and runs each NRO in a fresh canonicalized save home.
The signed copy avoids entitlement invalidation by concurrent Cargo builds.
Defaults: 15 seconds per example, one frame, corpus in
`~/.local/share/soju/switch/examples`, proof in
`~/.local/share/soju/switch/proof/rust-examples`. Overrides:
`SOJU_SWITCH_EXAMPLES`, `SOJU_SWITCH_PROOF`, `SOJU_SWITCH_TIMEOUT`,
`SOJU_SWITCH_FRAMES`, `SOJU_SWITCH_FONT`. When present, the installed
`host-unicode-bmp.ttf` is copied into each isolated home (or use
`SOJU_SWITCH_FONT` to select a host fallback). The summary has `name|result|detail`; individual logs retain
the first unsupported call or other failure. Existing PNGs for each tested name
are removed before running, so a failed run cannot inherit an old capture.

`frame` means a guest-submitted frame was captured, even if a later error occurs;
`exit0` means guest process exit without a frame; `stop` covers errors and timeouts
before a frame. The ExitProcess SVC carries no C `main` return value, so `exit0`
is not proof of successful application initialization. Always read `detail` in longer runs. The harness exits successfully
after recording compatibility failures; build/signing failures abort it.

```sh
# Use an ad-hoc signed hv binary, or the existing signed desktop binary:
soju-switch-core --headless example.nro --frames 60 --capture first.png \
  --inputs inputs.json
```

`--capture` writes the **first** submitted frame by default.
`--capture-frame N` selects a later submitted frame, with `N <= --frames`. Reaching the frame budget stops
the harness successfully and flushes its filesystem; it is not guest ExitProcess
or applet teardown. Headless execution never creates a window, polls host input,
or opens an audio device. Audio packets are consumed silently; this is not an
acoustic verification mode. `--inputs` is an optional JSON array:

```json
[
  {"at_ms":0,"buttons":1,"left":[-12345,23456],"right":[3456,-4567]},
  {"at_ms":400,"buttons":1024}
]
```

Times are increasing wall-clock milliseconds since guest execution begins.
States persist until replaced; omitted sticks center. Button masks use libnx
Npad bits (A=1, Plus=1024). Stick values are -32767..32767; stick-direction bits
are derived automatically. No host keystrokes are sent. Use a large frame budget
when testing timed input and graceful guest exit; a fast guest may otherwise
reach its budget before an input timestamp.

## Per-example comparison

The baseline is the supplied `proof/baseline-unicorn/summary.txt`. The Rust
comparison uses `--frames 1` with a 15-second timeout. The original 32 PNGs were inspected against the installed example sources.
Follow-up captures additionally verify Pacific time, curl cleanup, and the
supplemental shared-font example. Names below map to source directories by
replacing `__` with `/` under `~/devkitpro/opt/devkitpro/examples/switch`.
No capture exists for `stop`/`exit0` rows, so their intended output remains
unverified. Frame observations include error screens explicitly.

| Example | Unicorn | Rust | Source comparison / limitation |
| --- | --- | --- | --- |
| `account` | stop | frame | UID 1 and nickname Soju, as requested by the profile example; local offline identity. |
| `alarm-notifications` | stop | stop | Alarm registration/list UI blocked by missing notif:a. |
| `app_controldata` | stop | stop | Application-name lookup blocked at ns:am2 command 7989; no control data supplied. |
| `applet__app-playstats` | stop | stop | Play-statistics UI blocked by missing pdm:qry after selecting the local user. |
| `applet__libapplets__error` | stop | frame | Expected error-example banner; interactive error dialog not implemented/verified. |
| `applet__libapplets__pctlauth` | stop | frame | Expected pctlauth banner; next initialization stops on pctl:a. |
| `applet__libapplets__software-keyboard` | stop | frame | Expected swkbd banner; later keyboard creation stops on unsupported applet selector. |
| `applet__libapplets__web` | stop | frame | Expected web-example banner; web applet launch is not implemented. |
| `applet__light-sensor` | stop | frame | Expected sensor instructions; no sensor-reading acceptance. |
| `applet__lockexit` | stop | frame | Expected exit-lock banner; scripted Plus exits after the source’s five-second cleanup sleep. |
| `applet__recording` | frame | frame | Uniform RGBA (4,4,4,4), exactly the first shade computed by source; recording not verified. |
| `applet__screenshot-overlay` | stop | frame | Banner plus initialization error 0x1159; copyright overlay is not working in library-applet mode. |
| `applet__vrmode` | stop | stop | VR controls blocked by missing pctl:a; no VR acceptance. |
| `audio__audren-simple` | stop | stop | Renderer/sample-playback UI blocked by missing audren:u. |
| `audio__echo` | stop | stop | Microphone echo blocked by missing audin:u. |
| `audio__playtone` | stop | frame | 48 kHz, stereo S16 initialization/start succeed; expected tone controls. Scripted A then Plus exits; headless audio is silent. |
| `exception-handler` | stop | frame | Expected “Triggering crash...” banner; deliberate fault then stops at UnexpectedGuestException, not the guest exception handler. |
| `fs__romfs` | stop | frame | Both embedded text files printed through EOF, including the Unicode path; content matches the two source RomFS files. |
| `fs__save` | stop | frame | Local UID is available; mount reports 0x202 for the example’s absent external-title save. Not a save-mount success. |
| `fs__sdmc` | stop | frame | Expected directory listing includes the virtual soju-current.nro and Done. |
| `graphics__deko3d__deko_basic` | stop | stop | Triangle unverified: Modify and fixed big-page mappings now pass; next stop is kickoff flags 0x4. |
| `graphics__deko3d__deko_console` | stop | stop | GPU console/colored text unverified: current CPU GPFIFO submission contract rejects its submission. |
| `graphics__deko3d__deko_examples` | stop | frame | Expected nine-item software console selector. Scripted A selecting Simple Setup stops at CPU GPFIFO submission contract. |
| `graphics__opengl__dynamic_resolution` | stop | stop | Dynamic-resolution scene unverified: GPU syncpoint operation rejected during submission. |
| `graphics__opengl__es2gears` | stop | stop | Gears unverified: GPU syncpoint operation rejected during submission. |
| `graphics__opengl__gpu_console` | stop | stop | VT52 GPU console unverified: GPU syncpoint operation rejected during submission. |
| `graphics__opengl__lenny` | stop | stop | Lenny mesh unverified: GPU syncpoint operation rejected during submission. |
| `graphics__opengl__simple_triangle` | stop | stop | Triangle on teal background unverified: GPU syncpoint operation rejected during submission. |
| `graphics__opengl__textured_cube` | stop | stop | Textured cube on blue background unverified: GPU syncpoint operation rejected during submission. |
| `graphics__printing__hello-world` | stop | frame | Hello World at row 16, column 20 as specified. |
| `graphics__printing__vt52-demo` | stop | frame | Expected cursor-position labels and eight colored text/attribute rows. |
| `graphics__sdl2__sdl2-simple` | stop | stop | Red/green/blue rectangles unverified: keyboard/mouse activation accepted; next stop is HID SetNpadJoyHoldType command 120. |
| `graphics__simplegfx` | frame | frame | Uniform RGBA (4,4,4,4), exactly the first shade computed by source; 60-frame run also completes. |
| `graphics__simplegfx_moviemaker` | stop | exit0 | Returns before drawing; source has an early setup-failure exit. Guest exit status does not establish MovieMaker success; movie capture not verified. |
| `hid__abstracted-pad` | stop | stop | Virtual-controller controls blocked by missing hid:dbg. |
| `hid__colors` | stop | frame | Expected controller color fields, both zero for the virtual controller; physical colors not verified. |
| `hid__hdls` | stop | stop | Virtual-controller setup blocked by missing hid:dbg. |
| `hid__irsensor` | stop | stop | IR image unverified: missing irs service. |
| `hid__notification-led` | stop | stop | LED controls blocked by missing hid:sys. |
| `hid__read-controls` | stop | frame | Expected centered sticks; separate script displays exact left/right values and A down/held, then exits on Plus. |
| `hid__ring-con` | stop | frame | Instructions plus ringconCreate 0x1759 error; Ring-Con input not working. |
| `hid__sevensixaxis` | stop | stop | Console motion readings unverified: HID command 306 unsupported. |
| `hid__sixaxis` | stop | stop | Motion readings unverified: HID StartSixAxisSensor command 66 unsupported. |
| `hid__touch-screen` | stop | frame | Expected touch-position heading with no contacts; touch injection not implemented. |
| `hid__vibration` | stop | frame | Expected controls and initial frequency/amplitude values; physical rumble not verified. |
| `jit` | stop | frame | jitCreate reports 0x4759; does not run the example’s generated function returning 7. |
| `misc__barrier` | stop | frame | Expected Creating threads first screen; 60-frame run completes. |
| `misc__user_events` | stop | frame | Expected Creating thread first screen; 60-frame run completes after about five seconds. |
| `misc__user_timers` | stop | frame | Expected Creating thread first screen; 60 frames complete in 15.14 seconds with a 30-second timeout. |
| `network__curl` | stop | frame | Offline DNS failure and cleanup match source; 60 frames and scripted Plus exit verified. No host network request. |
| `network__ldn` | stop | stop | Local wireless UI blocked by missing ldn:u. |
| `network__lp2p` | stop | stop | Peer-to-peer UI blocked by missing lp2p:app. |
| `network__nxlink_stdio` | stop | frame | Expected hello/argv/host output; host is 0.0.0.0, no nxlink connection verified. |
| `nfc` | stop | frame | Expected NFC scan banner; later stops on nfp:dbg; no NFC data used. |
| `power` | stop | stop | Battery fields unverified: psm command 0 unsupported. |
| `settings__get_system_language` | stop | frame | en-US, enum 1, and conversion to ja all match source. Offline language list is limited to Japanese and US English. |
| `templates__application` | stop | frame | Expected Hello World at top left. |
| `time` | stop | frame | Host Pacific time/date and TZ=<PST>+08:00:00 verified by capture; LocationName is America/Los_Angeles, with DST-aware TZif conversion. |
| `usb__usbhs` | stop | stop | USB enumeration unverified: missing usb:hs service. |

## Longer runs and input checks

At the earlier checkpoint, a second corpus run requested 60 frames with the same 15-second timeout. It
retained 32 frame / 1 exit0 / 26 stop classifications, but revealed failures after
first frame in parental-control authentication (`pctl:a`), software keyboard
(unsupported applet selector), NFC (`nfp:dbg`), and the deliberate exception test
(`UnexpectedGuestException`). The timer example exceeded 15 seconds because its
source deliberately waits through three timer demonstrations; a separate
30-second run completed 60 frames in 15.14 seconds.

Scripted controller input displayed left stick (-12345, 23456), right stick
(3456, -4567), A down/held and the derived stick-direction state, then Plus
exited normally after 212 frames. Playtone accepted A and then Plus, exiting
after 309 frames. Lockexit received Plus and completed its five-second cleanup
sleep before exiting after 121 frames. Selecting the first deko menu entry with
A still stops at the GPU submission contract; the menu capture is not a GPU pass.

Proof lives under `~/.local/share/soju/switch/proof/`:

- `baseline-unicorn/summary.txt`: supplied before table.
- `rust-examples-before/summary.txt`: initial Rust result, also 2 frames / 57 stops.
- `rust-examples/`: final first-frame summary, logs and 32 PNGs.
- `rust-examples-60/`: longer-run summary, logs and captures.
- `rust-inputs/`: scripted controller/deko probes and extended timer run.
- `rust-examples/regressions/`: preserved regression and negative-test logs.

## Regression validation

All checks passed on the final code, without opening the app or a guest window:

| Check | Result |
| --- | --- |
| Cargo core, `hv` | 90 passed; signed tests run separately below |
| Cargo core, `desktop` | 92 passed; signed tests run separately below |
| Signed Hypervisor tests | 22 passed |
| Signed 2048 homebrew | Three moves, save, restart and graceful exit passed |
| Signed audio output | CoreAudio callback test passed |
| GPU crate | 17 passed; benchmark intentionally ignored |
| Shader crate | 43 passed |
| Filesystem crate | 50 passed, including TZif tests |
| Python full suite | 327 run, OK, 3 skipped |
| Clippy, desktop/all targets | Passed with warnings denied |
| App build and strict signature verification | Passed |

At the earlier checkpoint, the 2048 check measured 774.1 fps Rust versus 28.5 fps Unicorn for 60 steady-state
frames. These are local regression measurements under concurrent host load,
not a controlled comparison with the earlier 546 / 25.7 fps run.

The RGB565 test was also run against the previous decoder and failed before
passing with the fix. Reverting the exit-lock state in a temporary negative
check likewise made the grace-period test fail (5 seconds instead of 15).

## Local implementation commits

| Commit | Change |
| --- | --- |
| `a18987d` | Windowless CLI, first-frame PNG, timed input, corpus script |
| `83379d8` | Binder RGB565 conversion and opaque RGBX |
| `c672954` | Offline settings language API |
| `f4e90b1` | Local account application clients and profiles |
| `baf728b` | Current libnx audout auto-select commands |
| `5d55dc1` | Preselected offline user in loader configuration |
| `575bb93` | GPU topology query, cacheable mappings and 128 KiB pages |
| `781591f` | Applet exit-lock cleanup grace period |
| `90bbcc8` | Isolated signed executable for corpus runs |

## Remaining work

M2 remains incomplete. The default runner retains the bounded CPU path.
`SOJU_SWITCH_GPU=metal` selects the new synchronous boundary: map guest-backed
allocations, route subchannels to the imported engine, execute/read back Metal
work, and stage changed guest pages through the existing reply transaction.
Completion is not published before guest writes commit; a failed Metal operation
or failed publication makes that channel unusable. A native 32×32 red clear test
checks all pixels and verifies guest memory remains unchanged until publication.
This is a synthetic integration test, not example acceptance.

The bridge still lacks shader-extent registration, complete engine/MME routing,
general submission flags, semaphore scheduling, mapping/remapping support, and
persistent resource tracking across submissions. It currently rebuilds backend
memory per submission and synchronously reads back each action. It must remain
opt-in until these gaps and the example image checks are resolved.

The full Metal probe is in `rust-examples/gpu/summary.txt` and individual logs:
all six OpenGL examples stop on `902d:0260=0` (unsupported 2D engine method);
deko_console stops on submission flags `0x4`, fence `0:0`; deko_basic stops on
allocation ownership in a later mapping. The only PNG there is the CPU deko menu,
not an accepted GPU capture. Triangles, textures, cube, gears, and all selected
deko scenes remain unverified. No GPU/shader source was edited in this pass.

Audren and microphone input remain absent. Physical controller peripherals,
motion, touch injection, system applets, guest exception handling, JIT, external
save mounting, recording and the services listed in the table remain incomplete.
No feature is considered implemented merely because its error screen renders.
Snakey NX is deferred.

## Provenance

The corpus and example sources are the user's locally installed, signature-verified
devkitPro packages. No proprietary system data was supplied or added. Only the
installed example sources/headers and direct public libnx source references were
consulted for this pass; no general Switch web search was used.

Direct references came from `https://raw.githubusercontent.com/switchbrew/libnx/master/nx/source/`:
`services/set.c`, `services/acc.c`, `services/hid.c`, `services/audout.c`,
`services/applet.c`, `nvidia/ioctl/nvhost-ctrl-gpu.c`, and
`nvidia/ioctl/nvhost-as-gpu.c`. SHA-256 inventories of the downloaded reference
files, local example source files and all 59 NROs are preserved in
`rust-examples/*-sha256.txt`. Binaries, references and captures remain outside
tracked source files.

## Follow-up evidence and local commits

The signed 2048 follow-up again passed movement/save/restart/exit, measuring
209.17 Rust fps and 10.29 Unicorn fps under concurrent build/test load. These are
regression observations, not a controlled speed comparison. The signed audio
callback consumed all 47,920 expected nonzero samples.

The final 60-frame run used a 30-second timeout and retained 33 frame / 1 exit0 /
25 stop classifications. Previously documented post-frame applet/NFC/exception
failures remain; the time and curl examples complete all 60 frames.

- `rust-examples/time-zone/time.png`: Pacific clock, Tuesday November 3 at
  23:32:26, `TZ=<PST>+08:00:00`. UTC POSIX clock values remain unchanged.
  LocationName/load-rule/calendar IPC use a snapshot of `/etc/localtime`;
  opaque rules contain host TZif bytes and are accepted only by this service.
  Tests cover both 2024 DST boundaries and a footer-only future date.
- `rust-examples/offline/curl-cleanup.png`: “Couldn't resolve host name” and
  “cleanup” at frame 60. `curl-exit.png` plus the saved log verify normal guest
  exit after scripted Plus. BSD returns ENETDOWN, DNS returns EAI_AGAIN, and
  TLS context creation is unavailable; there are no host sockets or DNS calls.
- `rust-examples/supplemental/graphics__shared_font.png`: English sentence,
  `ファイル`, and `test Test`, matching source using the installed host Unicode
  subset. All 60 frames complete. No proprietary font data is used.
- `rust-examples/regressions-followup/`: core, native clear, signed execution,
  backend, Python, Clippy, app-build and curl-exit logs.
- `rust-examples/followup-reference-sha256.txt`: direct public libnx reference
  hashes. Supplemental NRO hashes and build logs are in `supplemental/`.

| Supplemental example (outside the 59) | Result | Remaining limitation |
| --- | --- | --- |
| `graphics/shared_font` | frame, 60 frames | Host font fallback verified; glyph coverage is that of the installed subset. |
| `audio/hwopus-decoder` | stop | Builds after correcting pkg-config prefix in an isolated copy; missing hwopus service. |
| `audio/sdl2-audio` | stop | Builds after the same correction; missing audren:u. |
| `graphics/sdl2/sdl2-demo` | not built | SDL2_image package is absent locally. |

The local supplemental builds use the unchanged installed example sources and
`pkg-config --define-variable=prefix=$(PORTLIBS)` in their copied Makefiles.
They do not replace the original 59-program corpus. Audren, audin, hwopus,
MovieMaker, peripherals, the remaining services, and previously documented error
screens are still unfinished; a displayed error is not feature acceptance.

| Local commit | Change |
| --- | --- |
| `809f272` | Merge current shader/GPU branch |
| `9009007` | Merge filesystem/TZif branch |
| `907234b` | Host timezone, location and opaque rule conversion |
| `4149fa5` | Opt-in transactional Metal boundary and native clear test |
| `724def5` | Select a later frame for capture |
| `c9889e2` | Retain C32 colour mapping kind |
| `03ececa` | Offline network errors, curl cleanup and host randomness |
| `c9d9d93` | Empty virtual keyboard/mouse activation |
| `018f103` | Unicode fallback for Standard font and isolated corpus assets |

## M2 mapping follow-up

MapBufferEx Modify (`0x100`) now changes kinds within an existing mapping
without looking up the deliberately zero nvmap handle. Repeated subrange changes
retain the original mapping identity and guest backing. Fixed/cacheable mappings
(`0x5`) must fit a vacant, aligned reservation; big-page padding is inaccessible.
The signed deko_basic probe reaches kickoff flags `0x4`, fence `0:0`; no GPU
frame is accepted yet. The corpus rerun remains 33 frame / 1 exit0 / 25 stop.
NV tests: 23 passed; desktop/all-target Clippy passed with warnings denied.

References: installed libnx `nvidia/ioctl.h` (Modify and FixedOffset flags),
`nvidia/gpu_channel.h`, and [NV service layouts](https://switchbrew.org/wiki/NV_services).
Local probe/test evidence: `dist/m2-probe/`.
