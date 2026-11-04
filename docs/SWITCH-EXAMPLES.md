# Official devkitPro example corpus

Local verification on 2026-11-03 (Pacific), starting at `6d00d6f` on
`feat/switch-core`. All changes are local commits; nothing was pushed.

**First-frame result: Unicorn 2 / 59; Rust 32 / 59.** Rust additionally exits
via the guest exit path without a frame for one example; 26 stop before a frame. These are
execution milestones, not 32 fully supported applications. **M2 is incomplete:**
the runner still does not route GPFIFO draws through the merged Metal/shader
backend. No GPU-rendered example frame has been verified. The deko example
selector is a CPU console, not evidence of a working GPU.

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
`SOJU_SWITCH_FRAMES`. The summary has `name|result|detail`; individual logs retain
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

`--capture` writes the **first** submitted frame. Reaching the frame budget stops
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
comparison uses `--frames 1` with a 15-second timeout. All 32 PNGs were inspected
against the installed example sources. Names below map to source directories by
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
| `graphics__deko3d__deko_basic` | stop | stop | Triangle unverified: NV MapBufferEx 0xc0284106 rejects color-memory kind 0xdb (C32_2CRA). |
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
| `graphics__sdl2__sdl2-simple` | stop | stop | Red/green/blue rectangles unverified: HID ActivateKeyboard command 31 unsupported. |
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
| `network__curl` | stop | stop | HTTP example blocked by missing ssl service; no request made. |
| `network__ldn` | stop | stop | Local wireless UI blocked by missing ldn:u. |
| `network__lp2p` | stop | stop | Peer-to-peer UI blocked by missing lp2p:app. |
| `network__nxlink_stdio` | stop | frame | Expected hello/argv/host output; host is 0.0.0.0, no nxlink connection verified. |
| `nfc` | stop | frame | Expected NFC scan banner; later stops on nfp:dbg; no NFC data used. |
| `power` | stop | stop | Battery fields unverified: psm command 0 unsupported. |
| `settings__get_system_language` | stop | frame | en-US, enum 1, and conversion to ja all match source. Offline language list is limited to Japanese and US English. |
| `templates__application` | stop | frame | Expected Hello World at top left. |
| `time` | stop | frame | Expected UTC time/date plus TZ and exit instructions; 60-frame run completes. |
| `usb__usbhs` | stop | stop | USB enumeration unverified: missing usb:hs service. |

## Longer runs and input checks

A second corpus run requested 60 frames with the same 15-second timeout. It
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
| Cargo core, `hv` | 86 passed; signed tests run separately below |
| Cargo core, `desktop` | 88 passed; signed tests run separately below |
| Signed Hypervisor tests | 22 passed |
| Signed 2048 homebrew | Three moves, save, restart and graceful exit passed |
| Signed audio output | CoreAudio callback test passed |
| GPU crate | 9 passed, including 4 native Metal tests |
| Shader crate | 43 passed |
| Python full suite | 325 run, OK, 3 skipped |
| Clippy, desktop/all targets | Passed with warnings denied |
| App build and strict signature verification | Passed |

The 2048 check measured 774.1 fps Rust versus 28.5 fps Unicorn for 60 steady-state
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

M2 remains unimplemented at the runner/backend boundary. GPFIFO still executes
the bounded CPU command subset. Mesa reaches submission but its syncpoint
operation is rejected; deko_console and a selected deko example fail the CPU
submission contract, while deko_basic first needs the C32_2CRA mapping kind
(0xdb). The merged GPU/shader crates passing synthetic/native tests does not
establish support for these real command streams. Metal resource residency,
shader translation of the examples, ordered execution/readback and presentation
must be integrated and verified before claiming M2.

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
