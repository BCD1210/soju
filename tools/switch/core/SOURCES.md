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
