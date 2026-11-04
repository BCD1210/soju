# Official devkitPro example corpus

## M2 method continuation (2026-11-04)

No GPU scene is accepted. The eight selected headless probes and ordered method
outcomes are tracked in [method-progress](../proof/rust-examples/gpu/method-progress/summary.txt);
[the method ledger](../proof/rust-examples/gpu/METHODS.md) supersedes the earlier
165c/0de4 blocker descriptions below.

- Deko Example01–04 and deko_basic: B197:2310=0x418800 (`SET_FALCON04`),
  whose effect is not described by the consulted B197/A097 headers.
- OpenGL simple_triangle, textured_cube and es2gears: initialization completes,
  then MapBufferEx rejects kind 0x17 (`NvKind_S8Z24_2CZ`, compressed depth/stencil).
- Point-center/watchdog fields, bounded setup retention, MME scratch forwarding,
  and isolated B1C0 setup/counters/inline uploads are covered by regression tests.
  Retained state still undergoes backend validation at the first rendering action.

The selected deko entries are independently scripted (A after 100 ms, with Down
press/release pairs for subsequent entries). Example01 is a scissored-clear scene;
Example02 is the triangle. Menu captures remain rejected and are not committed.
Scissored clears, shader extent registration, compressed-depth handling, and the
Falcon effect remain unfinished. No scene pixel acceptance test can pass yet.


Latest M2 follow-up starts from pushed checkpoint `61bce81`. GPU work through
`e633b45` from `feat/switch-shader` is merged, including the native command ring,
draw variants and aligned uniform heap. Core changes and merges in this pass are
local only; nothing was pushed.

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
| `graphics__deko3d__deko_basic` | stop | stop | Triangle unverified: mappings pass; default CPU path rejects flags 0x4. Metal reaches B197:165c initialization. |
| `graphics__deko3d__deko_console` | stop | stop | GPU console/colored text unverified: CPU rejects kickoff; Metal reaches B197:165c initialization. |
| `graphics__deko3d__deko_examples` | stop | frame | Expected nine-item software console selector. Selected Simple Setup reaches B197:165c under Metal; no accepted scene. |
| `graphics__opengl__dynamic_resolution` | stop | stop | Dynamic-resolution scene unverified: CPU syncpoint operation rejected; Metal reaches B197:0de4 initialization. |
| `graphics__opengl__es2gears` | stop | stop | Gears unverified: CPU syncpoint operation rejected; Metal reaches B197:0de4 initialization. |
| `graphics__opengl__gpu_console` | stop | stop | VT52 GPU console unverified: CPU syncpoint operation rejected; Metal reaches B197:0de4 initialization. |
| `graphics__opengl__lenny` | stop | stop | Lenny mesh unverified: CPU syncpoint operation rejected; Metal reaches B197:0de4 initialization. |
| `graphics__opengl__simple_triangle` | stop | stop | Triangle on teal background unverified: CPU syncpoint operation rejected; Metal reaches B197:0de4 initialization. |
| `graphics__opengl__textured_cube` | stop | stop | Textured cube on blue background unverified: CPU syncpoint operation rejected; Metal reaches B197:0de4 initialization. |
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

The bridge still lacks shader-extent registration, complete engine routing,
general submission flags, semaphore scheduling and persistent resource tracking.
It rebuilds backend memory per submission and synchronously reads back each
action. It must remain opt-in until the remaining gaps and image checks pass.

The current Metal probe is in `rust-examples/gpu/m2-followup/summary.txt`.
Deko basic/console and selected Example01 reach `b197:165c=0`; all six OpenGL
examples reach `b197:0de4=0x17`. These are initialization failures before an
accepted scene. Only the CPU menu is captured; it is not GPU acceptance.
No source under `tools/switch/gpu` or `tools/switch/shader` was edited locally.

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

## M2 submission and initialization follow-up

The core Metal bridge accepts hardware kickoff `0x4` with no requested fence
increment, routes MME upload/calls through the bounded interpreter, and counts
actual syncpoint increments. Zero-increment initialization does not invent a
completion. The existing `0x104` profile still requires one matching increment.
Writes and completion remain unpublished until the reply transaction commits.

MME now supports all eight result destinations, six-bit method increments,
conditional branch exits and an exit in a branch delay slot. Existing bounds,
unsupported ALU checks and rejection of reads after emitted methods remain.
Native clear tests exercise a macro-generated clear and two increments; the
source guest scene is still unaccepted.

902D notification-address and render configuration are retained. A tested 2×2
pitch RGBA8 SRCCOPY stages the exact source pixels without modifying guest memory
before publication. Enabled clipping, color key, zeta, conditional rendering and
bilinear sampling fail explicitly. B197 initialization covers disabled CSAA/MSAA,
alpha controls, interface-version checks and disabled window clipping. This is
partial engine coverage, not general 2D/3D support.

| Acceptance target | Current result | Next observed blocker |
| --- | --- | --- |
| deko Example01 Simple Setup | Not accepted | `B197:165c=0` |
| deko Example02 Triangle | Not accepted | Not reached after Example01 |
| deko Example03 Cube / Example04 Textured Cube | Not accepted | Not reached after Example01 |
| deko_basic / deko_console | Not accepted | `B197:165c=0` |
| OpenGL simple_triangle | Not accepted | `B197:0de4=0x17` |
| OpenGL textured_cube | Not accepted | `B197:0de4=0x17` |
| OpenGL es2gears | Not accepted | `B197:0de4=0x17` |
| OpenGL lenny | Not accepted | `B197:0de4=0x17` |
| OpenGL gpu_console | Not accepted | `B197:0de4=0x17` |
| OpenGL dynamic_resolution | Not accepted | `B197:0de4=0x17` |
| audren-simple | Not accepted | `audren:u` remains absent |
| SDL2 simple | Not accepted | HID command 120 remains unsupported |

Example01's installed source uses scissored clears: dark green background,
a centered 400×400 pale box, and green tiles inside. It does not draw the
triangle (Example02 does). A press held from time zero is consumed by the menu's
initial pad update; the selected-scene probe uses A at 100 ms and a large frame
budget. `deko-example01.log` records the actual selected-scene stop.
`rejected-menu-probe.png` was inspected and rejected as GPU evidence.
**There are no accepted GPU example captures or acceptance commits.**

Latest checks: core hv 98 passed; core desktop 100 passed; signed Hypervisor
22 passed; signed 2048 input/save/restart passed; signed AudioUnit tone passed
(47,920 nonzero samples consumed); GPU 25 passed / 2 benchmarks ignored; shader
44 passed; Python full suite 327 run, OK / 2 skipped. Desktop/all-target Clippy
passes with warnings denied; the app build and strict signature check passed.
Full corpus rerun remains **33 frame / 1 exit0 /
25 stop**, versus Unicorn **2 frame / 57 stop**. Logs are under `dist/m2-probe/`;
corpus and GPU logs are under the normal proof directory.

Core implementation commits: `bdc37aa` (mapping kinds/fixed big pages),
`36f8de7` (MME result destinations/delayed exits). See
[the source ledger](../tools/switch/core/SOURCES.md) for the public register and
ISA references. Audren routing, SDL completion, accepted triangles/cubes/textures,
shader extent tracking, broader engine initialization and persistent core-side
GPU resources are not done.

## Current GPU acceptance checkpoint

This supersedes earlier acceptance tables above. Shader integration includes
`feat/switch-shader` through `4481d03` (merges `3631bb5`, `a62cb55`).

| Scene | Result | Evidence / current boundary |
| --- | --- | --- |
| deko Example01 Simple Setup | **Accepted** | 921,600 rectangle/color pixel checks. |
| deko Example02 Triangle | **Accepted**, `c26e9a3` | 912,978 source-derived pixels; translated VS/FS, SPH and vertex state. |
| deko Example03 Cube | **Accepted**, `9196bd8` | 912,096 initial-pose pixels; uniforms, quads, real depth and logged ZCULL metadata. |
| deko Example04 Textured Cube | **Accepted**, `935c670` | 851,810 initial-pose pixels; B0B5 DMA, BC1 texture and translated TEXS. |
| OpenGL simple_triangle | **Accepted**, `20d7007` | 918,400 source-derived pixels; A140 `01b0=1001` uploads. |
| deko Example05 Tessellation | Not accepted | B197 `0320=0201`: tessellation parameters/stages unsupported. |
| deko Example06 Multisampling / Example07 MeshLighting | Not accepted | MSAA/CSAA render profile unsupported; compressed mappings now retained. |
| deko Example08 DeferredShading | Not accepted | B197 viewport 1 scale `0a20`; multiple-viewports not integrated. |
| deko Example09 SimpleCompute | Not accepted | B1C0 `02b4=02000a20`; core compute launch/window descriptor integration incomplete. |

[Captures, hashes and reproduction](../proof/rust-examples/gpu/accepted/README.md).
[Validation](../proof/rust-examples/gpu/accepted/VALIDATION.md).
[Method table](../proof/rust-examples/gpu/METHODS.md).
[Later-scene diagnostics](../proof/rust-examples/gpu/later-scenes/summary.txt).
Older scene-progress logs are historical. No later deko scene, other OpenGL
scene, or instancing scene is claimed accepted. Textures and uniforms are
exercised by accepted Examples03/04; compute and tessellation remain unfinished.

The final default-backend corpus rerun remains **59: 33 frame / 25 stop /
1 clean exit**, under `dist/task10-examples-final/`. Metal acceptance is separate.
The runner remains synchronous; merged async compilation/disk cache support does
not establish persistent resources or runner throughput. Tests force synchronous
pipeline compilation. Native scenes run serially because concurrent compilation
can advance animated cubes beyond the initial-pose oracle. No tolerance was widened.
All changes are committed locally, with no push and no installed-runner replacement.
