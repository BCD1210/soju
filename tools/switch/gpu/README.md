# Soju Switch GPU

`soju-switch-gpu` is a clean-room B197 register model and headless Metal backend.
It translates original Maxwell shaders using `../shader`, and uses `../texture`
for block-linear layouts and Metal format values. The texture crate is unchanged.
There are no registry dependencies, game binaries, firmware or keys. Public
references are in `SOURCES.md`; method provenance is in `REGISTERS.md`.

## Runner integration and API changes

1. Map allocations in `memory::Memory`. Every CPU/transfer write must use
   `Memory::write` so overlapping resources observe the new generation.
2. Register exact shader extents with `Backend::program(address, length, Options)`.
3. Feed decoded `(class, byte_offset, value)` methods to `Engine::method`.
   FIFO method indices must be multiplied by four; FIFO decoding is caller-owned.
4. Immediately forward each returned snapshot to `Backend::prepare`, before
   subsequent writes. Resources are captured at preparation, not submission.
5. `Backend::submit(directory)` queues a batch and returns `worker::Fence`.
   Empty submissions are valid ordered fences. `Fence::wait` waits for GPU
   completion without copying attachment bytes back to the CPU.
6. At a guest-memory visibility boundary, call `Backend::readback(directory,
   &mut memory)`. This waits for queued work, writes pending colour/depth/stencil
   and query results to guest memory, and returns RGBA8 colour previews.

Existing `program`, `prepare`, and `execute` signatures are unchanged. `execute`
is the compatibility wrapper: submit, wait, readback. `Action` adds
`ResetOcclusion` and `ReportOcclusion { address }`; downstream exhaustive matches
must handle these or keep forwarding snapshots to `prepare`.

**The runner must call readback before CPU reads, transfer copies, or sampling
pending attachments.** `Memory::read(&self)` cannot invoke the GPU service.
Unsynchronized CPU writes over pending attachment/query ranges are detected at
readback and rejected instead of overwritten. Changing a captured target via a
later prepared snapshot observes the newer write generation. Pending overlapping
views and simultaneous aliased MRT attachments are rejected.

Use an exclusive caller-owned directory for each Backend. Per-submission
subdirectories contain native build artifacts; readback still writes output files.
An eight-slot atomic ownership ring drives an in-process Swift Metal worker. Its
library is compiled once per process with `xcrun swiftc -O -g`, checked against the
Rust/C struct sizes, and retained for the process lifetime because Swift runtime
metadata cannot safely be unloaded. Each Backend owns its own device-side caches
and worker instance. Commands and resource captures cross the ring as owned typed
allocations, without JSON, resource-file uploads, or command serialization.
Failed requests poison the worker:
discard the Backend after failure rather than replaying possibly executed work.

`sync::Host` decodes A06F semaphore byte methods separately from B197. Equality,
unsigned greater-or-equal, and bitwise-AND acquires return readiness; the runner
must suspend/retry the guest channel while false. Four-byte WFI release waits the
preceding fence before writing its payload. `sync::Syncpoint::complete` increments
a wrapping completion counter after a successful fence; the runner supplies guest
syncpoint-ID routing. Neither operation performs image readback. Sixteen-byte,
timestamp and non-WFI releases are rejected. This is not a guest FIFO scheduler.

Requires macOS 15+, Xcode command-line tools and a compatible Metal GPU. Mac tests
fail if the GPU/compiler or required format support is missing. Other platforms
run portable tests only. Registers initially read zero as a software policy, not
an assertion about hardware reset values; callers must configure their state.

## Implemented execution

| Area | Coverage |
| --- | --- |
| Colour targets | Up to eight CT_SELECT outputs, arbitrary routing, independent RGB/alpha blending and separate-alpha factors; per-selected-target full-channel clears |
| Target storage | Pitched or block-linear colour; block-linear depth/stencil with block-height log2 0–5; one layer; checked extents, upload, load/store and explicit writeback; padding preserved |
| Colour formats | All 17 colour entries of the texture table, including sRGB, packed normalized, R32Uint and 16/32-bit float; guest writeback preserves precision; RGBA8 reports are previews |
| Depth/stencil | Z16, ZF32, Z24S8 promoted to float32/stencil8, ZF32_X24S8; imported guest contents, independent clears, compares, writes, front/back stencil operations/masks/references |
| Persistence | Attachment keys include address space, GPU address, format, dimensions and layout; writes invalidate affected contents; depth/stencil survives batches and supports first use without a clear |
| Textures | All 43 texture-table entries: normalized/float/packed, BC1–BC7, ASTC block sizes, sRGB, depth and unsigned integer; block-linear 2D, one mip/layer, checked pools |
| Texture views | TIC component permutations and zero/typed-one constants use Metal texture views; BGRA is expressed by component selection; packed-format channel order is adapted |
| Depth sampling | Guest depth unpacked to scalar R32Float; D16 normalized, D24 unpacked, D32 preserved; stencil sampling is not implemented |
| Integer sampling | All-integer tables specialize to `texture2d<uint>`; nearest sampling preserves raw register bits; integer target exports use `uint4`, clears preserve integer payload bits, and blending is rejected |
| Samplers | Point/linear min/mag, repeat/mirror/clamp-to-edge; normalized 2D; argument-buffer residency |
| Shaders/constants | Vertex B and fragment stages; explicit normalized texture handle mappings; bounds-checked shader extents, cbuf groups/binding/upload; limits inherited from the shader translator |
| Vertex fetch | Float1/2/3/4 and normalized RGBA8; checked streams, offsets, stride and limits; per-instance divisors and base instance |
| Draws | BEGIN/END, array/indexed draws, 8/16/32-bit indices, signed base vertex, instance sequencing; points, lines/line strip, triangles/triangle strip; arbitrary indexed restart splitting |
| Raster | Signed XY viewport scales, both clip-depth conventions and programmed depth range, viewport-0 scissor, winding/culling, fill/wireframe, fixed point size, eight shader clip distances |
| Polygon offset | Format-dependent bias/slope/clamp mapped to Metal; direction verified, cross-device bit precision not claimed |
| Queries | ZPASS reset and one-word REPORT_ONLY at DEPTH_TEST; Metal visibility counters count passing samples and write results at readback |
| Render enable | Explicit TRUE/FALSE; memory-backed predicate modes are rejected |
| Transfers | A140 single pitched-line upload; 902D pitched RGBA8 SRCCOPY rectangles, overlap-safe source capture; caller supplies render/transfer ordering |

For one-instance guest draw commands, divisor attributes are selected explicitly
at `floor(base_instance / divisor)` and bound with constant Metal stepping. This
avoids Metal's different base-instance/step-rate convention while retaining the
draw's base instance. NVIDIA A1R5G5B5 storage is repacked for Metal's A1BGR5 order.
D24 depth is promoted while GPU-resident and rounded to 24 bits on guest writeback;
that does not reproduce D24 quantization at every depth test.

## Current throughput

The mixed-state follow-up removes JSON and upload files, adds validated draw
variants, incremental native state groups, persistent tier-2 argument heaps,
per-slot uniform offsets, and cached render-pass descriptors. Disjoint color-only
passes may be grouped while preserving each target's draw order; depth, query,
clear, overlapping-target and generation boundaries prevent unsafe regrouping.
Public APIs remain unchanged. See [PERFORMANCE.md](PERFORMANCE.md) for the
before/after table, Instruments measurements, correctness evidence, and limits.

## Caches and batching

Pipeline keys cover translated shader sources, target formats/routing, vertex
layouts and blend/depth/stencil state. Target addresses and draw counts are dynamic.
Some state is conservatively included in keys. Native shader functions, pipelines,
depth states, textures and attachments survive submissions. Texture preparation
uses the full TIC and overlapping memory-write generation; unrelated writes hit.

Compatible consecutive draws share a render encoder and command buffer. Independent
color target sets are grouped within dependency-safe spans. Clears and actual
attachment changes end passes. CPU upload invalidation and query boundaries flush
GPU work. Disabled scissor restores full-target bounds. Diagnostics report pipeline,
depth-state and texture cache hits, encoder/command-buffer counts, worker seconds
and draws/second. Reusable graphics caches have completion-boundary LRU eviction (see Resource budgets). Range generations use split intervals,
so repeated writes to the same cbuf do not grow a write journal.

## Original foundation verification on Apple M4 Pro

```sh
cargo test --offline -- --nocapture
MTL_DEBUG_LAYER=1 cargo test --offline -- --nocapture
cargo clippy --offline --all-targets -- -D warnings
cargo test --offline --test render microbenchmark -- --ignored --nocapture
```

Tests hand-encode their own Maxwell programs, method streams and resource data.
All raster checks use translated guest shaders on `MTLCreateSystemDefaultDevice`.
The full suite also passes with Metal API validation enabled: 17 tests, plus the
separately run benchmark (18 total). Shader regression tests: 43 passed. Texture
regression tests: 7 passed. All three crates pass offline Clippy with warnings denied. Artifacts and logs
are under ignored `target/*-checks`, `target/benchmark`, and `target/final-*.log`.

| Image/readback group | Checked pixels |
| --- | ---: |
| Original clear/triangle/RGBA8/BC1/depth/blend/indexed-instance/scissor/cbuf/stencil | 10,192 |
| Eight routed targets with independent blending | 8,192 |
| Cross-batch depth/stencil, semaphore release and deferred readback | 2,048 |
| 17 colour formats, clear/draw and guest storage | 17,408 |
| 40 texture formats and swizzles, including nonzero sRGB/depth/packed samples | 40,960 |
| D16 and promoted D24/stencil rejection | 2,048 |
| Viewport/clip/point/restart/divisor/bias/query | 6,144 |
| Depth import, CPU invalidation and subsequent rendering | 8,192 colour + 8,192 raw depth/stencil |
| R32Uint exact-bit sampling | 1,024 |
| BGRA and BGRA sRGB sampling | 2,048 |
| Benchmark final image | 1,024 |

Total including the explicitly run benchmark: **107,472 pixel checks**, including
8,192 exact raw depth/stencil checks and 1,024 exact integer-sampling checks. The
triangle omits 48 edge samples. Normal colour tolerance is two channel units;
5-bit packed formats allow five. BCn/ASTC cases use constant blocks; they do not
constitute exhaustive decoder or filtering conformance. Query testing independently
asserts exactly 1,024 passing samples and excludes occluded fragments.

The 1,000-draw warmed benchmark uses one encoder and one command buffer, with
1,000 pipeline hits and 1,000 depth-state hits. Measured native-worker throughput:
**6,681 draws/s** (0.150 s). Including Rust preparation, translation, resource-file
writes and submission: **834 draws/s** (1.198 s). Compilation/warmup and final
readback are excluded. This measures a small headless workload, not game performance
or GPU-only throughput. Re-run the command to measure the current machine/load.

## Remaining gaps

- Memory-backed conditional rendering, other report types/sizes/timestamps,
  automatic FIFO/channel scheduling and guest syncpoint-ID integration.
- Runner-owned readback interception remains mandatory. No IOMMU or automatic transfer barriers; readback still uses output files.
- Mixed integer/float texture tables, mixed TIC component types, stencil sampling,
  mip/layer/cube/3D views, MSAA, anisotropy, custom borders and LOD bias.
- Shader-selected point-size slots, cull distances, viewport arrays, depth bounds/
  clamp, depth-format-independent bias, logic operations and D24-exact raster depth.
- Vertex A pairing, tessellation, geometry, transform feedback, indirect/packed
  draws, split-primitive BEGIN modes and quads/fans. Compute coverage is below.
- Vertex integer/half/scaled/packed formats beyond normalized RGBA8; generic
  attribute defaults; partial-channel/scissored/layered clears and full write masks.
- Pitched target width is byte pitch divided by pixel size; independent logical
  width/padding is not modeled. Transfer engines lack conversion/scaling/block-linear
  copies/ROP/color-key paths, and inline upload is single-line only.
- Many raster, sampler and blend combinations have mappings but no dedicated
  image test. Shader limitations in `../shader/README.md` and `SEMANTICS.md` remain.

This is a verified backend foundation, not a claim of game compatibility.

## Previous throughput baseline (048a31d)

Existing public structs and signatures remain compatible. The additive
`Engine::method_into(class, offset, value, &mut memory, &mut backend)` borrows
state directly instead of allocating a `Snapshot`. The original `method` plus
`prepare` remains supported and is used for the comparison benchmark below.
`Backend::enable_profiling` and `preparation_profile` expose opt-in timings for
state/attachment validation, shader/pipeline-key construction, and resource
capture/serialization respectively; repeated cached draws bypass these phases.

Method lookup is indexed. Shader translations are cached by address-space and
range generation, pipeline variants by exact framed state, and resource files
are deduplicated with collision-checked byte equality. Identical consecutive
validated draws are run-length encoded, preserving each actual Metal draw and
its blending effects. Every memory write or state change invalidates this fast
path; public register-map mutation is still checked. The fast path does not
claim incremental tracking for arbitrary changing graphics state. Query draws
are excluded. A seven-submission blending/cbuf test exercises ring wraparound.

Metal argument tables retain their textures and samplers across submissions.
Constants are captured into one 256-byte-aligned upload stream per submission
and copied once into one of three reusable Metal buffers. Slots are reused only
after completion. The worker still waits at each batch boundary; this is safe
triple storage, not three concurrently executing submissions. Native buffers and
function lookups are shared within a batch. Attachment changes still end passes.

Reproduce the four mixes with:

```sh
cargo test --offline --release --test render throughput_state_mixes -- --ignored --nocapture
```

The fixture uses 2,000 six-vertex, full-target draws at 32×32; changes occur on
every draw, alternating two texture pools, updating a 16-byte cbuf, or alternating
two targets. It warms both states, includes method decoding, snapshot preparation,
serialization, submission and GPU completion, and checks every final pixel.
Compilation and final readback are excluded. The source baseline is `2b31d37`
compiled in release mode, independently of the earlier debug-profile 834/s result.
Logs under `target/mixes-before.log` and `target/mixes-after.log` include decode,
prepare, file submission, worker JSON parse and encoding/completion timings.

At this historical baseline, the 300,000 draws/s requirement was met only for
repeated identical state. Mixed state was a blocker: changing-state preparation
allocated and captured attachments and resources, and the worker consumed
JSON/dictionaries and rebound state per draw. An incremental typed command/resource protocol and an
in-process encoder path remain necessary work; no retail throughput is claimed.

| Mix | Baseline end-to-end draws/s | Updated end-to-end draws/s | Updated worker draws/s |
| --- | ---: | ---: | ---: |
| Same state | 1,862 | 470,473 | 1,405,852 |
| Texture pool switch per draw | 1,646 | 13,757 | 30,013 |
| Cbuf update per draw | 1,620 | 16,175 | 45,520 |
| Target switch per draw | 1,907 | 10,914 | 18,934 |

Measured on this Apple M4 Pro in release mode. The worker columns exclude JSON
parsing, Rust work and file submission; end-to-end columns include all of them.
Metal validation passed 19 tests (12 raster and 7 portable); two benchmarks are
explicitly ignored in the normal suite. The mixed benchmark checks 5,120 final
pixels; the seven-batch repeat/ring regression checks 7,168 pixels.

## B1C0 compute integration

Additive API; graphics `Engine`, `Snapshot` and `Action` are unchanged:

```rust,ignore
let mut compute = soju_switch_gpu::compute::Engine::default();
backend.compute_program(address, code_length, options)?;
if let Some(dispatch) = compute.method(0xb1c0, byte_offset, value, &memory)? {
    backend.prepare_compute(&dispatch, &memory)?;
}
let fence = backend.submit(directory)?;
fence.wait()?;
backend.readback(directory, &mut memory)?;
```

Compute program extents contain scheduling bundles, without a graphics SPH.
Options must supply the shader ABI's one bounded `GlobalMemory` window (up to
64 MiB); this is an explicit runner-resolved allocation, not a GPU virtual-memory
implementation. QMD 0.6/1.7 supplies the program offset, three-dimensional grid,
local dimensions, up to 48 KiB shared storage, local allocations and eight cbuf
bindings. The shader crate lowers the kernel including LDS/STS and BAR. Native
device limits are checked before dispatch. The backend limits grids to 16,777,216
threadgroups. Unsupported QMD modes and B1C0 methods fail explicitly.

Code, QMD and constants are captured at preparation. Writable windows remain
GPU-resident between ordered dispatches, including across submissions. A fence
makes no guest-memory writes. `readback` copies the window into `Memory`; it does
not invent a width/height or add entries to `Report.images`. Synthetic tests
interpret that output as a 32×32 RGBA image. CPU writes over pending output,
overlapping windows, writable aliases of code/QMD/cbuf, and mixed graphics/compute
without a readback boundary are rejected. The runner must not CPU-read pending
GPU-written code, descriptors or outputs before readback.

The Metal validation test checks 4,096 pixels: both QMD versions execute a
cross-lane shared-memory exchange, and two independent windows with identical
initial bytes remain independent. Repeated dispatches read and accumulate prior
GPU writes. It also checks deferred visibility and CPU-write rejection. Portable
tests cover 40-bit addresses, cbuf fields, malformed dimensions, unsupported
versions, queue/semaphore/dependent/resume/numerical modes and invalid PCAS bits.

Remaining compute gaps: texture/image instructions, multiple global allocations,
indirect/queued/dependent launches, semaphore release, atomics, additional memory
widths, scheduling controls and automatic graphics/compute resource barriers.
This is a standalone-grid subset, not general B1C0 compatibility.

## Geometry and transform feedback boundary

Geometry remains rejected at draw preparation with `geometry stage/OUT emulation
is not implemented`. The shader crate rejects all reachable OUT register,
immediate and cbuf forms, including all four decoded stream-mode bit patterns.
B197 stream-output buffer/control/layout methods fail with a transform-feedback
error before changing engine state. These configuration methods are rejected even
when written with zero; applications that initialize disabled stream-output state
will need a future explicitly validated disable/configuration subset. Manually
constructed snapshots cannot bypass the rejection. Existing signatures remain
unchanged; only unsupported-path diagnostics become more specific.

A future implementation would need a compute prepass that executes vertex and
geometry invocations, records variable-length per-stream vertices/primitives,
and supplies a vertex-pulling raster pass. Transform feedback would need the
same ordered exports with guest buffer bounds, strides, write pointers and query
semantics. The present compute path has neither geometry input/primitive assembly
nor validated OUT export-handle/stream semantics. Adding vertex pulling alone
would not establish those semantics. No emulation is claimed or silently elided.

Final verification for this follow-up: **22 GPU-crate tests passed** with
`MTL_DEBUG_LAYER=1` (including compute); **44 shader tests** and **7 texture tests**
passed offline. Both ignored throughput tests were run explicitly during the
performance step. All three crates pass offline Clippy with warnings denied;
GPU and shader formatting checks pass. Evidence stays in ignored `target/`.

Current follow-up verification: **27 GPU tests** pass with Metal validation (the
original 22 plus five new image regressions); both ignored benchmarks also pass
with validation. Shader tests: **44**; texture tests: **7**. All three crates pass
offline Clippy with warnings denied. The five new regressions check 7,168 pixels.

## First-seen render variants

Set `SOJU_GPU_ASYNC_PIPELINES=1` before creating a Backend to compile MSL and
render pipelines on a serial utility queue. A missing/pending pipeline **skips
that draw**, including its depth/stencil/query contribution; clears still execute.
No incompatible pipeline is substituted and skipped draws are not replayed.
Fence diagnostics include `skipped_draws`. Ready results publish under a lock;
compiler jobs own source/descriptor copies, never borrowed command memory.
There are at most 32 pending jobs and 32 unconsumed results. Compiler errors
surface when their variant is requested again. Compute remains synchronous.
The default remains blocking to preserve existing `execute`/readback behavior
and the runner's first-scene integration. No public signatures or ABI changed.

Render pipelines use versioned, device/OS-scoped Metal binary archives keyed by
the stable pipeline key, exact MSL and compiler options. Archives live in
`target/soju-metal-cache-v1` relative to process cwd, overridden with
`SOJU_GPU_CACHE_DIR`. `SOJU_GPU_DISK_CACHE_BYTES` defaults to 128 MiB; zero disables
persistence. SHA-256 envelopes reject corrupt payloads before Metal reads them;
atomic replacement prevents partial publication. Cache failures fall back to
compilation. Oldest files are removed after publication to enforce the disk
budget. MSL still goes through Metal's library compiler on a disk-cache hit.

## Resource budgets

`SOJU_GPU_CACHE_BYTES` sets the per-Backend native graphics cache budget (default
256 MiB). LRU eviction runs after GPU completion and release of per-batch
bindings. Textures, render targets, depth targets, argument tables/heaps and
render pipelines participate. Argument-table dependencies are released with
textures; cached render passes are invalidated with attachments. Dirty colour
and depth/stencil contents spill to an exclusive `target/soju-spill-*` directory
and restore only for the matching guest generation. Readback and worker teardown
remove the spill files. Spills are authoritative pending guest output, not a
reconstructible disk cache, so their disk size is not capped.

Memory-pressure warning/critical notifications halve the effective budget until
normal pressure returns. This is enforced at submission/readback boundaries,
not during a frame. Diagnostics expose `cache_bytes`, `cache_budget`,
`cache_evictions` and Metal `metal_bytes`. Pipeline/function storage is charged
64 KiB per render variant because Metal has no per-pipeline allocation-size API.
The budget bounds accounted reusable graphics resources, **not process RSS or
Metal driver allocations**, active submission working sets, compiler jobs or
compute buffers. A single frame may temporarily exceed it.

CPU translated-shader, pipeline-state and deswizzled-texture caches each use a
byte-accounted LRU with `SOJU_GPU_CPU_CACHE_BYTES` (default 16 MiB per cache).
Captured commands own their resource copies. These settings do not change the
public API; set them before creating Backends. Zero budgets exercise full
reconstruction. Run the explicit soak with:

```sh
SOJU_GPU_CACHE_BYTES=524288 cargo test --offline --release --test render resource_churn_soak -- --ignored --nocapture
```

It runs 1,800 seconds, churns textures across 64 render targets, checks every
pixel, and logs process RSS plus Metal allocation size every minute. Set
`SOJU_GPU_SOAK_SECONDS` for a shorter smoke check and `SOJU_GPU_SOAK_LABEL` to
separate output directories.

## Parallel pass encoding

`SOJU_GPU_PARALLEL_PASSES=1` opts into concurrent encoding for draw-only batches
with two to eight disjoint colour attachment sets. Each set gets its own worker
queue/command buffer and preserves original draw order. Reference commands are
remapped into the group; immutable captured resources and completed pipeline
objects can be shared, while upload storage, heap cursors and render descriptors
are isolated. Every worker joins before the fence completes or an error returns.
Clears, depth/stencil, queries, compute, shared/partially overlapping attachment
sets, generation changes and spilled attachments select the ordered path.
Diagnostics report `parallel_passes`. This conservative mode is opt-in because
small passes may cost more to partition than they save in encoding time.

## Shared-depth target switches

Compatible consecutive shared-depth draws now use one render pass with the
union of their colour attachments (up to eight). Guest draw order is unchanged.
Fragment colour attributes and per-attachment blend/write masks are remapped to
the union; the remapping and union formats extend the stable pipeline key.
Clear/query/compute boundaries, depth identity/generation, dimensions, colour
generation changes and the eight-attachment limit split groups. This also handles
MRT slot permutations without remapping an output twice. Set
`SOJU_GPU_MERGE_DEPTH=0` to compare the original separate-pass path.
