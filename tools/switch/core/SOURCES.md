# GPU integration public references

The runner uses installed libnx headers and the public references below. These
references describe wire layouts and instruction fields; guest examples come
from the installed devkitPro example package. No emulator sources are used.

- https://switchbrew.org/wiki/NV_services
  NVMap, address-space mapping and GPFIFO ioctl layouts.
- https://github.com/switchbrew/libnx/blob/master/nx/include/switch/nvidia/ioctl.h
  Installed header: MapBufferEx FixedOffset, IsCacheable and Modify flags.
- https://github.com/switchbrew/libnx/blob/master/nx/include/switch/nvidia/gpu_channel.h
  Installed header: GPFIFO entry flags and per-channel fence accumulation.
- https://raw.githubusercontent.com/envytools/envytools/f102b82381f3f11cee113d16374c87091db039d9/envydis/macro.c
  MME result destinations, method selector fields and branch/exit delay semantics.
  Hand-encoded tests exercise parameter/send ordering and conditional exit.
- https://raw.githubusercontent.com/envytools/envytools/f102b82381f3f11cee113d16374c87091db039d9/rnndb/graph/gf100_3d.xml
  CSAA_ENABLE, MULTISAMPLE_MODE, LINKED_TSC and window clip register descriptions.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/3d/clb197.h
  MME upload/call registers, alpha controls and interface version checks.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/twod/cl902d.h
  902D initialization: render enable, clipping, color key, beta values, processing
  clusters, corral size, overlap and sample mode. Actual copies continue through
  the GPU crate's bounded pitch RGBA8 SRCCOPY implementation. Unsupported clip,
  color key, zeta, conditional rendering and bilinear modes fail before copy.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/host/cla06f.h
  GPFIFO entry address/length, privilege, level and synchronization fields.

The Metal bridge remains synchronous. It counts executed increment methods,
accepts zero-increment initialization, and publishes completion only after guest
memory writes commit. This is not an asynchronous GPU scheduler. Unsupported
methods and instruction combinations remain explicit errors.

M2 continuation also consulted the corresponding public Kepler header:
https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/3d/cla097.h
Both A097 and B197 describe SET_FALCON04 (0x2310) only as V[31:0];
this is not enough evidence to implement its firmware-mediated effect.
B197 SET_MME_SHADOW_SCRATCH is ordinary 32-bit scratch storage. The MME
interpreter forwards its latest emitted scratch value to subsequent reads;
reads after emissions to other methods still fail explicitly.

- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/compute/clb1c0.h
  B1C0 local/shared windows, pool addresses and limits, reference-counter selector
  and value, and inline DMA layout/completion/barrier fields. Setup is isolated
  from the 3D engine; pitch/no-semaphore inline data uses the existing bounded
  upload engine. Scheduling/compute dispatch remains explicitly unsupported here.

Scene acceptance continuation uses public deko3d driver source at commit
`373bfd43d8b2d805fa2eb7d406dbb823032df958` (not emulator source):
- https://github.com/devkitPro/deko3d/blob/373bfd43d8b2d805fa2eb7d406dbb823032df958/source/maxwell/engine_3d.def
- https://github.com/devkitPro/deko3d/blob/373bfd43d8b2d805fa2eb7d406dbb823032df958/source/maxwell/gpu_3d_base.cpp
- https://github.com/devkitPro/deko3d/blob/373bfd43d8b2d805fa2eb7d406dbb823032df958/source/dk_swapchain.cpp
- https://github.com/devkitPro/deko3d/blob/373bfd43d8b2d805fa2eb7d406dbb823032df958/source/maxwell/clear_buffers.mme

These identify tiled-cache setup, shadow passthrough/replay around clears and
surface decompression, and the native swapchain format. B197's public header
supplies release-semaphore size/payload/order fields. Release writes are staged
transactionally after synchronous completion, never treated as no-ops.

The original local devkitPro `graphics/opengl/simple_triangle/source/main.cpp`
supplies the triangle/color oracle; no graphics-driver implementation is used.
NVIDIA clb197.h identifies the default line/stipple, viewport/scissor, depth-layer
and depth-bounds methods. libnx `nv/types.h` identifies compressed kinds 53/E0/E9.
A140 launch structure-size bit 12 has no effect with completion type FLUSH_ONLY;
nonzero semaphore-release completion remains unsupported by inline uploads.

The same pinned public driver revision supplies `utils.mme` (counted register
fill), `pgraph.mme` (scratch completion polling), and `program.mme` (per-stage
constant binding). Taken branches cancel exit in their delay slot. The runner
acknowledges only the exact masked context-configuration operations listed by
`gpu_3d_base.cpp`; other requests fail. This is a host-context model, not execution
of a firmware image. The old interpreter's early exit concealed both incomplete
five-group initialization and missing completion polling. Scissor bounds for
higher slots are retained and validated; viewport-index shader output is still
unsupported.

- https://github.com/devkitPro/deko3d/blob/373bfd43d8b2d805fa2eb7d406dbb823032df958/source/maxwell/utils.mme
- https://github.com/devkitPro/deko3d/blob/373bfd43d8b2d805fa2eb7d406dbb823032df958/source/maxwell/pgraph.mme
- https://github.com/devkitPro/deko3d/blob/373bfd43d8b2d805fa2eb7d406dbb823032df958/source/maxwell/program.mme

## Application boot probes

The self-built libnx titles in `../testtitles` exercise application proxy selection,
launch storage, offline account profiles, settings, clocks, fonts and HID calls.
Service command/layout references (consulted 2026-11-04):

- https://switchbrew.org/wiki/AM_services
- https://switchbrew.org/wiki/Account_services
- https://switchbrew.org/wiki/APM_services
- https://switchbrew.org/wiki/HID_services
- https://switchbrew.org/wiki/Settings_services
- https://switchbrew.org/wiki/Network_Interface_services
- https://switchbrew.org/wiki/Friend_services
- https://switchbrew.org/wiki/Parental_Control_services

The local profile has one user, no network connection and no parental restrictions.
Six-axis start/stop and stationary shared-memory samples are covered; host motion
input is not connected. Unsupported commands still stop explicitly. A successful boot probe
is not evidence that an untested application boots.

Additional boot-path references:

- https://switchbrew.org/wiki/Glue_services (ARP control property)
- https://switchbrew.org/wiki/BCAT_services (prepo transmission status)
- https://switchbrew.org/wiki/SSL_services (local context lifetime)
- https://switchbrew.org/wiki/Filesystem_services (save identities)
- Installed libnx public HID structure declarations (offsets checked by the probe compiler).


Parallel execution and code-memory continuation (2026-11-04):

- Installed Apple Hypervisor SDK `hv_vcpu.h` and `hv_vcpu_types.h`: thread-affine
  vCPU ownership, cross-thread exit requests, virtual timer exit/mask behavior,
  and the zero-offset relation to `mach_absolute_time()`.
- Installed Mach time and pthread QoS headers: timer conversion and worker class.
- https://switchbrew.org/wiki/SVC#CreateCodeMemory
- https://switchbrew.org/wiki/SVC#ControlCodeMemory
- Installed libnx `switch/kernel/svc.h` and public SVC assembly ABI: code-memory
  operations, permissions, and register arguments.

The code-memory implementation is a bounded single-process compatibility path:
heap backing, one writable owner alias, one read/read-execute alias, exact-range
unmaps, and final-handle cleanup. It permits both aliases in the same process;
modern cross-process ownership restrictions and handle transfer are not modeled.
This choice is explicit and is not evidence for cross-process JIT behavior.

## Control fence events

The register/unregister IDs and event query bit are described by the public libnx
API implementation: https://github.com/switchbrew/libnx/blob/master/nx/source/nvidia/ioctl/nvhost-ctrl.c
and https://github.com/switchbrew/libnx/blob/master/nx/source/nvidia/fence.c .
The synchronous runner accepts waits only for committed completion values; future
threshold waits remain unsupported, rather than returning premature success.

## Desktop power queries

The public libnx `nx/source/services/psm.c` command wrappers (consulted 2026-11-04)
and installed `switch/services/psm.h` define charge (0, u32), charger (1, u32),
raw charge (13, double), sufficient supply (14, bool), and battery age (15, double):
https://github.com/switchbrew/libnx/blob/master/nx/source/services/psm.c
The desktop profile reports constant full charge, full battery health and mains
power. These are virtual-device values, not host battery telemetry. Unimplemented
power-management commands remain errors.

## Read-only play history

https://switchbrew.org/wiki/Shared_Database_services#pdm:qry
defines HOS 17 query commands and event/statistics array layouts. The runner
currently records no play journal. Enumeration returns zero records without
writing caller arrays; range queries report an empty range. Input/output arrays
and flags are checked. Individual statistics commands 4/5 and journal recording
remain unsupported, rather than returning invented timestamps or launch counts.

## Read-only application control data

https://switchbrew.org/wiki/NS_services#GetApplicationControlData documents the
source byte, aligned application ID, receive buffer and u32 returned size.
https://switchbrew.org/wiki/Error_codes documents 0x410 as title ID not found.
The 7989 child exposes command 0 against the current launch's supplied 0x4000-byte
control property, using the same program identity as the filesystem. Unknown
identities and missing properties return 0x410 without output writes. The property
is resident for all three source modes. Icon loading and language-selection
commands remain unsupported.

## Empty USB host bus

https://switchbrew.org/wiki/USB_services#usb:hs,_usb:hs:a describes process binding,
16-byte filters, 0x228-byte interface records and three availability-event slots.
https://switchbrew.org/wiki/Error_codes supplies USB invalid-state, argument, size
and interface results. No host devices are exposed. Enumeration returns zero
records without writes. Availability/removal events stay unsignaled; filters and
event ownership are retained per session, duplicates are rejected, and destroy or
close releases the owned handles while guest duplicates survive. Acquiring any
interface fails with ResultInterfaceInvalid. Unimplemented commands still stop.

## Physical controller enumeration

https://switchbrew.org/wiki/HID_services#GetUniquePadsFromNpad and
https://switchbrew.org/wiki/HID_services#GetUniquePadIds specify the receive-list
array of u64 UniquePadIds and signed 64-bit output count. The runner's synthetic
Npad is not a physical controller, so both queries enumerate zero physical pads.
The documented 0x58aca result rejects invalid Npad IDs. No LED is fabricated;
direct LED writes remain unsupported.

TEXS array discovery preserves the shader crate's existing public-ISA type
decoding (mode 7) while resolving the packed cbuf texture/sampler handle. The
GPU backend validates the resolved type against the TIC and uploads all layers.
The gpu_console probe advances past its type mismatch to integer vertex input.

IR absent-camera profile: [HID services](https://switchbrew.org/wiki/HID_services#irs)
and [result codes](https://switchbrew.org/wiki/Error_codes) define the lifecycle,
read-only 0x8000 status memory, nine 0xe30 camera slots, disconnected status 2,
and IrsensorNotReady (0xf0cd). The desktop has no IR camera: sample rings stay
empty and image/MCU queries fail without output writes. PID/resource ownership,
camera handles, output bounds and image-transfer memory handles are validated.
Other processor configurations remain explicit unsupported commands.

Seven-six-axis fusion follows the [HID service transfer-memory/ring ABI](https://switchbrew.org/wiki/HID_services#InitializeSevenSixAxisSensor).
The stationary desktop source retains both transfer handles, publishes +Z
acceleration and an identity quaternion, wraps a 33-entry ring, and freezes it
when stopped. Timestamp reset changes the timestamp origin, not sampling order.
Fusion strength is stored per session. Finalize/session close releases the pins.
The virtual console reports at-rest. AM VR begin/end/query retain the requested
mode per [CommonStateGetter](https://switchbrew.org/wiki/Applet_Manager_services#ICommonStateGetter);
there is no physical panel/backlight transition on the desktop.
HIPC's PID descriptor is supplied by the sender process at the service boundary,
independent of the client placeholder bytes ([HIPC](https://switchbrew.org/wiki/HIPC)).
The runner uses process 1 consistently with its GetProcessId SVC.

Notification record ABI: [Glue services](https://switchbrew.org/wiki/Glue_services#notif:a).
Application sessions share a bounded eight-record alarm store with 0x40-byte
weekly schedules and at most 0x400 parameter bytes. CRUD validates identity,
schedule fields, buffer bounds and capacity. The normal desktop launch has an
unsignaled AM notification channel. Records currently live for the runner's
lifetime; host wake/relaunch scheduling and cross-run alarm persistence are
not implemented. Undocumented error cases fail explicitly rather than returning
invented notification-module result values.

Audio input follows [Audio services](https://switchbrew.org/wiki/Audio_services#audin:u):
48 kHz PCM16 mono/stereo buffers complete in capture order, publish PCM before
signaling release events, and retain pending durations across stop/start.
Desktop capture uses Apple's AVAudioEngine and AVAudioConverter. Permission is
checked using AVCaptureDevice authorizationStatusForMediaType; a missing usage
description, denied permission, absent device or engine failure supplies silence.
A first permission request supplies silence until capture is restarted. Headless
runs never request microphone access. Captured samples are neither saved nor logged.
Native compilation and deterministic silent-queue tests are verified; live
microphone capture still requires a hardware/permission check.

HDLS follows [HID debug services](https://switchbrew.org/wiki/HID_services#AttachHdlsWorkBuffer)
and the public libnx `hiddbg.h` structures: retained RW transfer memory, distinct
session/device identities, dedicated FullKey devices in free player slots,
state-list dumps, and shared-memory buttons/sticks/colors/power publication.
Detach and session close clear those slots. Unsupported controller types, motion
attributes and assignment/state-list apply commands remain explicit errors.
Legacy AbstractedPad commands are unavailable in the advertised modern profile;
clearing their empty autopilot set does not detach independently owned HDLS devices.
Both public HID debug examples reached three frames; the state roundtrip and
shared-memory/transfer-lifetime test passed.

Offline local wireless uses the [LDN/LP2P interface definitions](https://switchbrew.org/wiki/LDN_services).
LDN cannot initialize a radio and remains State=None. Subsequent operations
return documented LDN NotInitialized (0x4acb). LP2P has the documented empty
Initialize/DestroyGroup behavior and remains role=0, link-level=0, with unsignaled
state/join events. Radio-dependent operations return generic NotSupported
(0xfe01, [public result table](https://switchbrew.org/wiki/Error_codes)). This is
an explicit desktop capability policy, not a claim that native hardware returns
that same result for every radio failure. No peers, addresses or successful
transmissions are fabricated. Module-specific unavailable-radio error mapping
remains unverified. Both public networking examples reached three frames.

### Four-sample rendering and resolve

The B197 MODE_2X2/sample-position controls and 902D centered bilinear copy
controls follow the NVIDIA public headers cited in the GPU source ledger.
The runner retains sample state through clear/draw normalization and flushes
attachment ownership when sample storage changes. Example06 is accepted by
`tests/gpu_msaa.rs` against its public source's initial cube transform, gradient
and four programmed sample locations (921,600 pixels, 1,580 coverage edges).

## Compute-generated smooth lines

Example09 acceptance derives its tick-zero geometry from the local public
sinewave.glsl and Example09_SimpleCompute.cpp. It checks the compute launch and
921,600 pixels after rendering, including line coverage and blending. Line AA
and width registers survive draw normalization. Inactive point/polygon AA bits
are accepted for line draws; polygon smoothing and wide aliased line draws
still reject. See the GPU source ledger for the public rasterization contract.

## Asynchronous desktop presentation

Apple's Metal guidance describes independently owned in-flight storage released
by command-buffer completion handlers:
https://developer.apple.com/library/archive/documentation/3DDrawing/Conceptual/MTLBestPracticesGuide/TripleBuffering.html
SDL's public SDL_Metal_CreateView/GetLayer/GetDrawableSize APIs retain native
window/input integration. The presenter uses three bounded upload textures,
per-slot ownership semaphores, a serial encoding queue, CAMetalLayer drawables
and completion-driven reuse. Drawable acquisition and presentation occur off the
runtime thread. Shutdown drains encoding and all submitted work. Resize changes
only a free slot's texture; active uploads remain owned until completion.

The old host-loop timer and SDL_RenderPresent wait are removed. Display sync is
provided by CAMetalLayer; temporary lack of a drawable (occlusion/minimization)
releases the slot without a presentation. Native command failures are sticky and
reported on subsequent submission or final drain. This host swapchain does not
relax guest attachment-writeback or syncpoint visibility requirements.

The native check queues 96 frames with alternating dimensions, overwrites/frees
caller memory immediately, resizes the window and verifies all three retained
uploads after GPU completion. It also exercises close/reopen and invalid calls,
with Metal API validation enabled. Run the ignored presenter integration test
on a logged-in macOS desktop.

## Terminal syncpoints after channel close

Switchbrew documents control syncpoint read/wait ioctls separately from channel
file descriptors: https://switchbrew.org/wiki/NV_services
The public host1x overview likewise distinguishes channels from synchronization
counters: https://docs.kernel.org/gpu/tegra.html
The runner's monotonic virtual syncpoint IDs previously reused the channel's
internal lookup entry, so closing a completed channel destroyed its counter.
Mesa's later cleanup wait then failed despite the fence already being complete.

The virtual policy now keeps only (session owner, terminal counter, fault flag)
after channel close. It keeps no executable channel, mapping or GPU resources.
Unknown IDs, cross-session reads, faulted waits and future thresholds still fail.
Session close removes the counters, and live plus retired entries are capped at
4096 before admitting another channel. This is a bounded virtual lifetime policy,
not a claim about the hardware allocator's ID reuse. A focused lifetime test and
120-frame es2gears/dynamic_resolution desktop runs pass, including orderly exits.
