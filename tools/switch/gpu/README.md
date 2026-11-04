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

Use an exclusive caller-owned directory for each Backend. Resources and manifests
use per-submission subdirectories. A bounded worker-thread queue drives a persistent
Swift Metal service; it is compiled once per Backend with the installed `xcrun
swiftc`. The device and caches survive batches. Failed requests poison the worker:
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

## Caches and batching

Pipeline keys cover translated shader sources, target formats/routing, vertex
layouts and blend/depth/stencil state. Target addresses and draw counts are dynamic.
Some state is conservatively included in keys. Native shader functions, pipelines,
depth states, textures and attachments survive submissions. Texture preparation
uses the full TIC and overlapping memory-write generation; unrelated writes hit.

Compatible consecutive draws share a render encoder and command buffer. Clears and
attachment changes end passes. CPU upload invalidation and query boundaries flush
GPU work. Disabled scissor restores full-target bounds. Diagnostics report pipeline,
depth-state and texture cache hits, encoder/command-buffer counts, worker seconds
and draws/second. Caches and the write journal currently have no eviction policy.

## Verification on Apple M4 Pro

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
- Runner-owned readback interception remains mandatory. Uploads still use files;
  no in-process Metal bridge, cache eviction, IOMMU or automatic transfer barriers.
- Mixed integer/float texture tables, mixed TIC component types, stencil sampling,
  mip/layer/cube/3D views, MSAA, anisotropy, custom borders and LOD bias.
- Shader-selected point-size slots, cull distances, viewport arrays, depth bounds/
  clamp, depth-format-independent bias, logic operations and D24-exact raster depth.
- Vertex A pairing, tessellation, geometry, transform feedback, indirect/packed
  draws, split-primitive BEGIN modes, quads/fans and compute dispatch.
- Vertex integer/half/scaled/packed formats beyond normalized RGBA8; generic
  attribute defaults; partial-channel/scissored/layered clears and full write masks.
- Pitched target width is byte pitch divided by pixel size; independent logical
  width/padding is not modeled. Transfer engines lack conversion/scaling/block-linear
  copies/ROP/color-key paths, and inline upload is single-line only.
- Many raster, sampler and blend combinations have mappings but no dedicated
  image test. Shader limitations in `../shader/README.md` and `SEMANTICS.md` remain.

This is a verified backend foundation, not a claim of game compatibility.
