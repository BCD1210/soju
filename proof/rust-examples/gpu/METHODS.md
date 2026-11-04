# M2 method progression

Sources: NVIDIA [clb197.h](https://github.com/NVIDIA/open-gpu-doc/blob/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/3d/clb197.h), envytools [gf100_3d.xml](https://github.com/envytools/envytools/blob/f102b82381f3f11cee113d16374c87091db039d9/rnndb/graph/gf100_3d.xml).
Offsets are byte offsets. `handled` means a method completed dispatch, not that a scene passed acceptance. `rejected` means dispatch/action failed. Original FIFO input and expanded MME dispatch are distinguished by `gpu-method` and `gpu-status` lines with `SOJU_SWITCH_TRACE_GPU=1`.

| Method | Public name and fields | Semantics and implementation | Probe outcome |
| --- | --- | --- | --- |
| B197:165c | SET_POINT_CENTER_MODE, V[31:0], OGL=0 / D3D=1; rnndb POINT_RASTER_RULES | Retain mode; reject other values. Point draws are explicitly blocked until centering is verified; triangles and clears are unaffected. | Deko passes this initialization; next 1684=1. |
| B197:0de4 | SET_SM_TIMEOUT_INTERVAL, COUNTER_BIT[5:0]; rnndb WATCHDOG_TIMER | Retain hardware watchdog selection, reject reserved bits. Host execution limits own scheduling; this does not emulate NVIDIA counter timing. | OpenGL passes; next 19cc=1. |
| B197:1684 | SET_PROVOKING_VERTEX, V[0], FIRST=0 / LAST=1 | Flat-interpolated attributes select first/last primitive vertex. | Retained with field validation; rendering effect remains gated. |
| B197:19cc | SET_Z_COMPRESSION, ENABLE[0] | Hardware depth compression enable. | Retained with field validation; rendering effect remains gated. |

No accepted scenes yet. Installed Example01 uses scissored clears; Example02 is the triangle. A software menu capture is not GPU proof. Per-attempt raw logs are under `dist/m2-probe/iterations/`.

## Continuation result

See [the full observed catalogue](method-progress/catalogue.md) and
[per-scene summary](method-progress/summary.txt). Ordered method outcomes are
retained per example in `method-progress/*.methods`. Reproduce with an isolated,
signed release runner and `python3 scripts/probe-switch-gpu.py <runner> --output <directory>`.

Deko Example01–04 and deko_basic now reach B197:2310=0x418800 after MME scratch
read-after-write. `SET_FALCON04.V[31:0]` is the only description in both pinned
B197 and A097 headers. Its effect is not established, and it is rejected.

OpenGL simple_triangle, textured_cube and es2gears now finish the initialization
submission, including separate B1C0 setup and reference counters and a staged
pitch inline upload. They next request MapBufferEx kind 0x17, page size 0x20000.
Installed libnx `nvidia/types.h` identifies this as `NvKind_S8Z24_2CZ` (compressed
depth/stencil), which the mapping allowlist does not support. The page size itself
is already supported. No compressed-depth interpretation was guessed.

B197 setup values are range-checked and retained for later reads and snapshots;
this does not implement all their rendering effects. The GPU backend still rejects
unsupported state at an action. B1C0 dispatch remains rejected. In particular,
provoking-vertex and compression setup passing is not flat-shading or compressed
surface acceptance. Scissored clears and shader-extent registration also remain
necessary before the target scenes can pass. There are no acceptance commits,
accepted captures, or headless scene pixel tests.

## Accepted Example01 continuation

Earlier rejection entries above describe the previous checkpoint. The observation
policy now supersedes the Falcon and compressed-kind stops.

| Method / field | Implementation and acceptance evidence |
| --- | --- |
| B197 2300–237c, 0140/0144/1114, 260c/33dc | Explicit firmware/performance/trap no-op allowlist; per-method counters and unconditional `gpu-noop` logging. No memory writes or invented fences. |
| B197 0100 | Public NO_OPERATION; counted/logged. |
| B197 0f64/0f68/0f6c/0f70/1108 | Public deko3d identifies tiled-cache setup; host Metal cache policy, accepted/logged no-ops. |
| B197 0124 | Track/pass/replay for explicit target, surface-clip, window-origin and multisample fields. Uninitialized or unclassified replay stops. |
| B197 0fbc–0fc8 | All samples enabled (`ffff`); other masks stop. |
| B197 0d9c / 0f40 | Disabled color reduction/constant-color rendering only. |
| B197 0ff4/0ff8 | Retained surface clip; full target in accepted scene. |
| B197 19d0, 0e00/0e04/0e08 | Native Metal scissored color clear; full-image source oracle passes. |
| B197 081c | Array pitch is irrelevant for one layer; native test verifies nonzero pitch. |
| B197 02e0 / 0f80 | Surface decompression/cache flush value zero completes synchronously under uncompressed-storage policy. |
| B197 1b0c=1000f010 | Real four-byte release payload, staged after preceding work; unknown write forms still stop. |
| A06F 002c=80000000/70000000 | L2 flush/invalidate satisfied by synchronous snapshot/readback. |
| Map kind 17 / 51 / db | Retained compressed S8Z24 / Z24S8 / C32 kinds; ordinary surface storage. |
| MME register ADD/SUB/XOR/ANDN/NAND | Public ISA operations; independent wraparound/emission tests. Carry operations remain unsupported. |

Example01 is accepted; see [capture and reproduction](accepted/README.md).
Its ordered method outcomes are saved alongside the capture. Passing setup alone
continues to mean only successful dispatch, not acceptance of other scenes.

Example02 continuation additionally validates disabled early-Z, post-Z mask,
interlock, render-target offset, fill-via-triangle, point/line/polygon smoothing,
alpha test and logic operation; full CT_WRITE masks only. Viewport clipping is
retained for the observed deko (`181d`) and OpenGL (`2`) profiles. These states
are **not yet translated at a draw**. Subtiling performance knobs 0360/0364 are
counted no-ops. Example02 reaches B197 0d78 (draw count), where backend preparation
rejects the retained setup snapshot (first diagnostic: 0110). Shader extent
registration also remains undone; this checkpoint does not bypass those checks.
Latest ordered outcomes and blockers: [scene-progress](scene-progress/summary.txt).

## Example02 draw integration

NVIDIA `clb197.h` supplies method names and fields. Explicitly validated disabled
iterated blend (`0xdd0`), array restart (`0xde8`), antialias (`0x1534`), viewport
enable (`0x192c`) and default blend options now pass the bridge. Viewport clip
rectangles must contain the active scissor; depth clip must remain [0,1].
`0xdd8` ZCULL criterion is an accepted/logged optimization no-op. The existing
validated setup is retained for macro reads, then excluded from raster snapshots
where the backend does not consume it. Unknown methods remain rejected.

The source ledger for IPA encoding is the shader crate's pinned envytools
`envydis/gm107.c` (`tabe000_0`, mode 1, operand registers 20 and 39). Arithmetic
was inferred from the installed original homebrew shader and tested with W=2;
no emulator/codegen source was used.

## Example03 depth and ZCULL

Allowed no-op metadata: `02e8/02f8/02fc`, `07c0/07c4/07c8/07cc/07e0/07e4`,
`0dbc`, `12c8`, `15c8/15fc/1600`, `1958`, `1978`. These never write depth
storage or report memory. `1464` STORE_ZCULL remains unsupported. `0de0`
PIXEL_SHADER_BARRIER value zero is satisfied by synchronous readback.
`139c` stencil mask participates in MME shadow track/replay for depth clears.
Single-layer ZT_ARRAY_PITCH does not change the selected layer's address.

Discard provenance: devkitPro/deko3d `source/maxwell/engine_3d.def` (word 0x3de),
`source/maxwell/gpu_3d_base.cpp`, `dkCmdBufDiscardDepthStencil`.
https://github.com/devkitPro/deko3d/blob/master/source/maxwell/gpu_3d_base.cpp

## Example04 transfers and texture descriptors

B0B5 `0400`–`041c`, `0700`–`0720`, `0728`–`073c` configure checked virtual
addresses, pitches, remaps and block-linear geometry; `0300` launches copies.
Unknown methods, physical addresses, semaphores, reductions, nonidentity remaps
and unsupported 3D layouts reject before destination publication.
B197 inline `0180/0184/0188/018c/0190/01b0/01b4` uses isolated upload state.

Sources: NVIDIA `classes/dma-copy/clb0b5.h`, `classes/3d/clb197tex.h`,
deko3d public header `dkMakeImageHandle`/`dkMakeSamplerHandle` (20-bit image
index and upper sampler index), and pinned envytools `tabd200_2` split TEXS
component masks. Native tests check distinct RGBA components; copied texture
bytes are validated separately with origins, padding and overlapping ranges.

## OpenGL triangle and later-scene boundaries

A140 `01b0=1001` uses the synchronous inline profile; bit 12 selects semaphore
structure size but has no write without a semaphore completion mode. The same
validation is shared by B197/B1C0 inline uploads. Release profiles still reject.
B197 shader/TIC/TSC cache invalidations (`021c`, `1330`, `1334`) and tiled-cache
enable (`0f60`) are logged no-ops under synchronous execution.

Default-only validation covers line width, disabled stipple/two-sided lighting,
identity coordinate swizzle, default higher-viewport depth/scissor initialization,
zero base vertex/depth layer and disabled depth bounds. Enabled unsupported
modes stop. Viewport clip intersects the active scissor before drawing.
Compressed kinds `53`, `e0`, `e9` retain their original values and ordinary backing;
this does not enable multisample rendering.
