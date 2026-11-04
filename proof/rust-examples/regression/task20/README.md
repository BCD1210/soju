# Task20 continuation

Base: `989b89c`. Implementation and verification commits remain local; no push.
The final quiet default corpus is **58 frames / 0 stops / 1 clean exit**, up from
44 / 14 / 1. All 59 processes returned status 0. The first stops have been
implemented, including the microphone
input queue and desktop capture bridge. Four-sample rendering/resolve and
Example09 now have source-derived pixel acceptance. Desktop presentation uses
an asynchronous, bounded three-frame Metal queue.

This task is **not fully complete**: tessellation remains 14/16 translated
programs, and the requested 17.5 ms p99 target has not been established. A frame
from the deko example menu does not count as tessellation scene acceptance.

## Implemented behavior

| Area | Behavior and evidence |
| --- | --- |
| audin:u | AVAudioEngine capture at 48 kHz stereo PCM16; explicit permission handling and silence when unavailable; sample-duration-based queue release, PCM publication before event, stop/resume and bounded buffers; native desktop build and echo-buffer timing tests pass. Live authorized microphone capture was not exercised. |
| notif:a | Validated, bounded in-memory alarm CRUD, owned event, empty application notification storage. Records last for the runner session; no host wake/relaunch. |
| ns:am2 7989 | Read-only current-application control-data child; missing application errors and output-capacity validation. |
| pdm:qry | Validated read-only queries over an empty virtual play journal. |
| hid:dbg | Virtual FullKey controllers, retained transfer-memory ownership and state publication; physical pad enumeration remains separate. |
| hid:sys | Empty physical UniquePadId enumeration; virtual Npad input remains available. |
| HID 306 | Stationary seven-six-axis data in owned transfer-memory rings, with event/timing and stop behavior. |
| irs | Disconnected camera state and no-image result; no fabricated camera image. |
| ldn:u / lp2p:app | Explicit offline virtual policy, no peers/link, unsupported radio initialization and operations. The generic unsupported result is not asserted to be a hardware-specific radio-offline code. |
| usb:hs | Empty bus, owned hotplug events and validated descriptor capacities. |
| deko_console | B197 FILL_BBOX expansion, alpha tests and bounded long MME loops; console glyph capture inspected. Unsupported vertex side effects/clip-crossing cases still reject. |
| gpu_console | TEXS array type and handle propagation through layered uploads. |
| Samplers | All eight comparison functions and decoded anisotropy limits. Native pixel checks cover comparison thresholds; no exhaustive PCF or array-comparison conformance claim. |
| MSAA | Four individual samples, color/depth/stencil storage, guest writeback/reimport and programmed positions. UNORM/sRGB color and supported float depth/stencil formats; centered 2x2 resolve averages sRGB in linear light. |
| Example06 | 921,600 source-derived pixels, including exact alpha for 1,580 partial-coverage edge pixels. |
| Example09 | Real compute launch followed by smooth line-strip rasterization; 921,600 source-derived pixels and 8,863 partial-coverage pixels at tick zero. |
| Desktop swapchain | Three upload slots, asynchronous drawable acquisition/encoding, completion-driven reuse and drain-on-close; resize/reopen/ownership validation for 96 native frames. Both 120-frame desktop probes exit cleanly. |
| Syncpoint lifetime | Completed counters survive channel close for their owning session; no executable channel retained. Future thresholds, faulted waits and cross-session access remain rejected. |

Public contracts and implementation limits are recorded in
[core SOURCES](../../../../tools/switch/core/SOURCES.md) and
[GPU SOURCES](../../../../tools/switch/gpu/SOURCES.md). The source-derived scene
tests and native checks are executable repository artifacts. Raw captures and
logs stay under ignored `dist/service-continuation/`.

## Validation

- Core desktop library: **142 passed**, no failures.
- Source-derived accepted scenes: **12 passed**, including the new Example06 and Example09 checks, with Metal API validation.
- GPU crate: **35 active render tests passed**, five existing ignored tests; four library, two compute, four copy, thirteen state tests and both native validation harnesses passed.
- Texture layout: **8 passed**, including 432 padding-preserving byte-address cases.
- Strict all-target core/GPU clippy passed; native Objective-C builds use `-Wall -Wextra -Werror`.
- Native presenter: 96 queued frames with immediate caller-buffer overwrite, resize and reopen; Metal validation passed.
- Desktop es2gears and dynamic_resolution: 120 frames each, both process exits 0, with Metal validation.
- A stale test that rejected sample-position register `0x11e0` was updated to test implemented state and reserved values; the complete 142-test run above is the subsequent clean run.

Reproduce the accepted scene checks with `scripts/test-switch-gpu-accepted.sh`.
Run `cargo test --offline --manifest-path tools/switch/core/Cargo.toml --features desktop --test presenter -- --ignored` for the native window check.
Run `scripts/test-switch-examples.sh` for the locally installed default corpus.

## Remaining gaps

- Tessellation control/evaluation stages lack an implemented patch ABI and native
  execution path. The fresh audit still decodes 521/521 instruction occurrences
  but translates 14/16 programs. ISBERD/per-vertex attribute addressing, patch
  constants, tessellation factors and wide smoothed tessellated wireframes need
  an end-to-end implementation and scene oracle. No translation-count shortcut
  or replacement scene is used.
- Asynchronous host presentation is implemented, but GPU submission/attachment
  readback remains synchronous for guest-memory coherence. Snapshot comparisons,
  color/depth writeback and guest-layout conversion remain significant costs.
- MSAA currently supports four samples, not every raster/storage combination.
  Example07 still reaches an inactive vertex-attribute boundary and is not
  accepted. General scaling, additional sample counts and alpha-to-coverage
  remain unsupported.
- Notification persistence/host wake, physical HID/IR/USB hardware, online local
  wireless transport and live microphone verification remain outside the proven
  virtual-device behavior above.
- Smooth lines currently require nonindexed single-sample segments and ordinary
  attributes, with bounded width/expansion; wide aliased lines and general
  polygon smoothing remain unsupported.

See [per-title results](per-title.md), [frame times](frame-times.md),
[shader audit](shader-audit.txt), and [local commits](commits.md).
