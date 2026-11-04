# Soju Switch GPU

`soju-switch-gpu` is a clean-room B197 register file and synchronous, headless
Metal backend. It consumes original method streams and uses
`soju-switch-shader` for Maxwell shader translation and `soju-switch-texture`
for block-linear uploads and color-target writeback. Neither dependency was
modified for this work. There are no registry crate dependencies.

This is an executable backend foundation, not a complete Switch GPU or a claim
of game compatibility. The worker runs real Metal commands on the default GPU;
it does not substitute software rendering or fixture-specific Metal shaders.

## Integration

1. Map guest GPU allocations into `memory::Memory`. All subsequent writes,
   including transfer-engine writes, must use `Memory::write` so cache
   invalidation sees them.
2. Register each shader allocation's exact address and extent with
   `Backend::program(address, length, Options)`. Bytes are reread and translated
   at draw preparation; replacing code at the same address changes its key.
   Extents must come from the runner, not a scan into adjacent guest memory.
3. Feed `(class, byte_offset, value)` to `Engine::method`. These are decoded
   methods, **not FIFO packets**. NVIDIA class headers use byte offsets; a FIFO
   word-method index must be multiplied by four before calling this API.
4. Immediately pass each returned `Snapshot` to `Backend::prepare` before any
   later memory writes. Shader, vertex, index, constant and texture data are
   captured at this point. `prepare` validates supported state and bounds.
5. Call `Backend::execute(directory, &mut memory)` at a synchronization boundary.
   Use an exclusive, caller-owned directory. It compiles the bundled Swift
   helper with `xcrun swiftc`, writes a local resource manifest, runs Metal,
   waits for completion, reads back color targets and writes them to guest
   memory in their declared layout. The returned report has RGBA images and
   device/cache diagnostics. Failed batches remain queued for diagnosis; discard
   the backend after a failed execution rather than blindly replaying effects.

Flush before CPU access to pending color targets, copies involving those targets,
using their results as vertex/index/constant data, or sampling them as textures.
Sampling a pending render target and overlapping target views are explicitly
rejected. Copies and inline writes execute immediately in `Engine::method`;
the caller owns synchronization with pending render batches.

The native helper is part of the backend, also used directly by the integration
test path. It uses `MTLCreateSystemDefaultDevice`, no window or display surface.
It requires macOS 15 or newer, Xcode command-line tools, and a Metal-capable GPU.
BC1 tests require device BC1 support; missing GPU/compiler/support is a test
failure on macOS. Non-macOS builds run the portable state tests only.

Registers start at zero as an explicit software initialization policy; these are
not asserted to be hardware reset values. Callers must set target layout,
shader enables, vertex formats, viewport and clip convention explicitly.

## Coverage

| Area | Implemented execution |
| --- | --- |
| Register file | Public method catalogue for B197, A140 and 902D; named byte offsets and bounded arrays in `REGISTERS.md`; unknown methods rejected |
| Color target | CT_SELECT target 0 only; A8B8G8R8/RGBA8 UNORM; block-linear or tightly pitched, single layer; upload, load/store, readback and guest-memory writeback |
| Zeta | ZF32 and ZF32_X24S8 Metal attachments; size validation; full clear required on first use; depth/stencil preserved between draws in the same batch |
| Viewport | Viewport 0, positive XY scales, top-left Metal coordinates; explicitly selected zero-to-positive-W clip depth; Z offset/scale maps to Metal near/far |
| Scissor | Enabled viewport-0 scissor within target bounds |
| Vertex fetch | Float1/2/3/4 and normalized RGBA8; stream stride, address, inclusive limit; per-instance stream divisors; used stream indices 0–29 |
| Shaders | Standalone Vertex B and fragment from program-region address plus stage offset; SPH stage checked; other enabled stages rejected |
| Constants | Five binding groups, 32 register slots; selector size/address, bind/unbind, sequential inline constant loading; actual shader use restricted to Metal-valid slots; vertex cbuf/stream collisions rejected |
| Textures | Maxwell 32-byte block-linear TIC; RGBA8 and BC1 UNORM identity components; normalized 2D, one level/layer, block-height log2 0–5; guest range validation and texture-crate deswizzling |
| Samplers | 32-byte TSC; point/linear min/mag, repeat/mirror-repeat/clamp-to-edge, no mip filtering; argument buffers with explicit residency |
| Handles | Existing shader normalized bindings select TIC and TSC pool indices independently, checked against pool limits; up to 16 resources per table; default paired-index ABI also supported |
| Draws | BEGIN/END, array first/count, indexed first/count, 8/16/32-bit indices (8-bit widened), signed indexed base vertex, instance first/subsequent/unchanged, global base instance |
| Topology | Points, lines, line strip, triangles and triangle strip; no restart or split primitives |
| Blend | Target-0 RGB/alpha operations and factors, separate alpha, constant blend color; dual-source factors rejected |
| Depth/stencil | OGL/D3D compares, depth writes, front/back stencil ops, masks and refs |
| Raster | Front winding, front/back culling, matching front/back fill or wireframe polygon modes |
| Clears | Full target color channels together; depth/stencil independently; no MRT/layer/partial-channel/scissored clears |
| A140 inline upload | One pitched line, checked destination and tail bytes; no semaphore/interrupt effects |
| 902D copy | Pitched nonlayered RGBA8 SRCCOPY rectangles, unity derivatives, integer origins; bounds checked before writes; source captured before destination writes for overlap |

Register storage is broader than execution support. A catalogue entry is not an
execution claim. Unsupported method effects, enabled stages, descriptor formats
and resource modes return errors when they are used; many raster/attachment
checks also run in the native worker before submission.

## Caches and lifetimes

Pipeline keys combine both shader keys with canonical descriptor state: target
formats, vertex formats/strides/divisors, blending and depth/stencil configuration.
Absent registers and explicit zero agree. Target/buffer addresses, clears,
draw counts and viewport/scissor are dynamic and do not create new pipelines.
Some stencil dynamic state is conservatively retained in the key. Native shader
functions are cached by exact translated source; depth/stencil states share the
pipeline's state key. Native caches live for one execution batch.

Texture preparation caches deswizzled data by the complete TIC and overlapping
memory-write generation. Unrelated writes remain cache hits; descriptor changes,
relevant writes and changing address spaces invalidate reuse. Native texture
objects are keyed by descriptor and content digest, so changed uploads cannot
reuse stale Metal contents. Color-target writeback advances memory generations.
Caches and the write journal currently have no eviction policy. `Memory` is a
checked mapped-allocation model, not an IOMMU implementation; a read must fit
within one mapped allocation.

## Verification

Run from this directory:

```sh
cargo test --offline -- --nocapture
cargo clippy --offline --all-targets -- -D warnings
```

The graphics test hand-encodes its own Maxwell shaders and builds its own method
streams, vertex/index buffers, TIC/TSC records and block-linear texture data.
Every draw uses translated guest shader code. The Swift helper is compiled during
the test, executes on Metal, waits for completion and returns RGBA8 readback.

Measured on Apple M4 Pro: 15 draws, 12 clears, 7 pipeline creations, 8 pipeline
cache hits, 2 texture uploads and 2 native texture-cache hits. Ten 32×32 images
are compared against independent expected pixel calculations:

| Image | Checked pixels | Maximum measured channel error |
| --- | ---: | ---: |
| Clear | 1024 | 0 |
| Interpolated coloured triangle | 976 | 0 |
| RGBA8 block-linear textured quad | 1024 | 0 |
| BC1 block-linear textured quad | 1024 | 0 |
| Depth rejection | 1024 | 0 |
| Separate-alpha blending | 1024 | 0 |
| Indexed plus two instances | 1024 | 0 |
| Scissor | 1024 | 0 |
| Constant-buffer mutation between snapshots | 1024 | 0 |
| Stencil write followed by equality test | 1024 | 0 |

Tolerance is two channel units out of 255. Only the triangle excludes 48 samples
near edges to avoid making a rasterization edge-rule claim; its interior uses
CPU barycentric interpolation, including checks of the clear background. All
other images check every pixel. RGBA and PPM artifacts, manifests, translated
MSL and the compiled worker remain under ignored `target/render-checks/`.
The triangle and indexed-instance artifacts were also visually inspected.

Portable tests cover draw sequencing, constant upload/binding, inline upload,
copy rectangles, malformed descriptors, range overflow, overlapping mappings,
dirty overlap/non-overlap and address-space changes. Draw preflight tests reject
bad stream/index limits, clip conventions, target/stage/attribute formats and
unregistered shader extents before native execution.

The existing shader and texture offline suites were also run unchanged.

## Remaining gaps

- No runner/FIFO/subchannel integration, macro interpreter, fences, semaphores,
  query/report handling, conditional rendering or compute dispatch in this crate.
  Transfer-engine/render ordering is the caller's responsibility.
- The Swift worker is a synchronous process per batch, recompiles its host helper
  at execution, and copies resources through files. There is no persistent
  in-process Metal service, asynchronous scheduling or bounded production cache.
- Zeta contents are device-only within a batch. They are not loaded from or
  written back to guest memory, nor preserved across `execute` calls; first use
  requires a clear. This prevents using depth as a texture or preserving it across
  batches. Color padding is reinitialized on block-linear writeback.
- No MRT, MSAA, layered/cube/3D/mipped or sRGB textures, texture swizzles beyond
  identity, depth sampling, anisotropy, custom borders, LOD bias or compressed
  formats beyond BC1. Pitched render-target width is treated as its full byte
  pitch divided by four; independent logical-width/padding is not implemented.
- No native packed bindless-handle decoding. The runner must supply the shader
  crate's explicit normalized handle mappings. No guest packing is guessed.
- No Vertex A pairing, tessellation, geometry, transform feedback, indirect or
  packed draw methods, primitive restart, quads/fans, negative viewport scales,
  negative-W clip depth, viewport arrays, depth bias/bounds/clamp or logic ops.
- Vertex integer/half/scaled/packed formats beyond normalized RGBA8 and generic
  attribute defaults are missing. Point/line, wireframe, culling, 8/32-bit indices,
  nonunit instance divisors and most blend/stencil combinations have mappings but
  do not yet have dedicated image tests. Only the scenarios above are GPU-verified.
- Full write masks, clear rectangles and partial clears need draw-based lowering.
  Inline upload is single-line pitched; 2D copy has no scaling, conversion,
  block-linear paths, ROP operations, clipping or color keying.
- Shader semantics remain limited by `../shader/README.md` and `SEMANTICS.md`.
  This backend does not claim equivalence beyond those translator guarantees.

Public references and fingerprints are in `SOURCES.md`; every catalogued method
has its public header link in `REGISTERS.md`. The pinned NVIDIA extraction script
is a development tool only; tests and builds never fetch sources.
