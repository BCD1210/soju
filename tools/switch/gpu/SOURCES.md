# Public source ledger

Only the public sources below were fetched. No emulator, firmware, game or keys are used.

NVIDIA open-gpu-doc pinned at `9fdf5c4062007929d9f4e6cbad9c9771fe61b880` (MIT; notice in LICENSE.nvidia).
Revision discovery: https://api.github.com/repos/NVIDIA/open-gpu-doc/commits/master
Tree discovery: https://api.github.com/repos/NVIDIA/open-gpu-doc/git/trees/9fdf5c4062007929d9f4e6cbad9c9771fe61b880?recursive=1

- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/3d/clb197.h
  SHA-256 `175c50f29798b39c6858cafe31497e9e9798c70ca068a64856719003b5d088d7`.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/3d/clb197tex.h
  SHA-256 `d5aad7d7f339e09411b40a5e8b996613ae2c6d802187070bfa66a596513151bf`.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/inline-to-memory/cla140.h
  SHA-256 `19a0d452641b2023fb208dd0599dec123006dd193955f53af6a86c73d9a98c31`.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/twod/cl902d.h
  SHA-256 `8fa8bcb9f69077cc67f21ff68b17d16078e1cf200be6d728502bbba99ecb83bc`.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/host/cla06f.h
  SHA-256 `6c5db0562c117a76f145acbe35a6bacafe612493424078439d1a83c627fbc0b8`.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/dma-copy/clb0b5.h
  SHA-256 `169d958bfa379bfdf24c4656f02265ff5bcf22a2866724ee4299138d29df5332`.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/3d/clb097.h
  SHA-256 `61babb743bc5cd8666906575aa7eeda313e0ddb5179714b5a867facc8d59e100`.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/3d/clb097tex.h
  SHA-256 `4dfcee5f7b09d9c3efba8c3dd07efb43b37d41586ae3d9338165413b19fe363c`.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/3d/cl9097tex.h
  SHA-256 `0b1bc445c0c46401769ec77fc8b97f9fae3c5644054ec7f847cf887d25141162`.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/compute/clb1c0.h
  SHA-256 `6d612ede54e9615a30c0e5f03f2dcaed3039e062a7eb329a482782e9eb473942`.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/compute/clb1c0qmd.h
  SHA-256 `0041719fd1619035e58eb882e04196743dc1475af65147ff851fdf9f81dd672c`.

B197 methods and field encodings: clb197.h; Maxwell TIC and TSC: clb197tex.h.
A140 inline upload: cla140.h; 902D copy: cl902d.h. Other headers were downloaded for comparison only.
Per-method links: REGISTERS.md. Original shader fixtures use the ISA sources already recorded in ../shader/SOURCES.md.

- https://developer.apple.com/documentation/metal
  Metal device, descriptors, render encoders and readback; locally installed SDK declarations are the API authority.

- https://raw.githubusercontent.com/envytools/envytools/f102b82381f3f11cee113d16374c87091db039d9/rnndb/graph/gf100_shaders.xml
  Export map PSIZE at 0x6c and CLIP_DISTANCE at 0x2c0–0x2dc; same pinned
  public SPH source as the shader crate. Apple MSL specification (in the shader
  ledger), Table 5.4, supplies Metal point_size and clip_distance attributes.
  Metal SDK MTLTexture.h supplies texture-view swizzles; MTLRenderCommandEncoder.h
  supplies visibility counting, depth bias, viewport, and vertex step functions.

The installed Apple SDK's `MTLBlitCommandEncoder.h` supplies buffer/texture copies
and the depth/stencil plane options used for attachment import and readback.

B1C0 execution now uses the already pinned `clb1c0.h` and `clb1c0qmd.h` above,
including PCAS scheduling, program offset, CTA raster dimensions and cbuf fields.
The installed SDK `MTLComputeCommandEncoder.h` and `MTLComputePipeline.h` define
dispatch, static threadgroup allocation limits and buffer bindings. No additional
third-party implementation was consulted.

Persistent argument heaps and offset-only bindings (Apple documentation, accessed
2026-11-04; Markdown copies retained under ignored `target/gpu4/`):

- https://developer.apple.com/documentation/metal/mtlargumentencoder/setargumentbuffer(_:offset:)
  Positioning encoded argument tables within a shared buffer allocation.
- https://developer.apple.com/documentation/metal/mtlrendercommandencoder/setfragmentbufferoffset(_:index:)
  Updating a bound buffer's offset without binding another resource; the analogous
  vertex method is declared in the installed `MTLRenderCommandEncoder.h`.

No guest register encodings or shader semantics changed in this throughput work.

Long-session work (2026-11-04), Apple public APIs:
- https://developer.apple.com/documentation/metal/mtlbinaryarchive
- https://developer.apple.com/documentation/metal/mtldevice/currentallocatedsize
- Installed macOS SDK Metal headers: MTLBinaryArchive.h, MTLDevice.h,
  MTLCommandBuffer.h, MTLRenderCommandEncoder.h.

MME variable extraction/shift fields are described by the pinned envytools
`envydis/macro.c` rows 3 and 4 (REG2 supplies the source or destination bit
position). Soju rejects shifts and bitfields outside 32 bits and tests both
forms with independent expected emitted values. The public libnx
`include/switch/nvidia/types.h` names mapping kinds 1/2 as Z16/Z16_2C; they use
the existing canonical uncompressed D16 storage path in this runner.

Viewport arrays (2026-11-04): NVIDIA's public Shader Program Header specification,
OmapSystemValuesB, orders ViewportIndex immediately before PointSize. Combined with
the public export address map this selects output 0x68. The pinned B197 header
specifies sixteen viewport transforms (stride 32) and scissors (stride 16).
https://download.nvidia.com/open-gpu-doc/Shader-Program-Header/1/Shader-Program-Header.html
Apple's installed MTLRenderCommandEncoder.h and MSL specification define
setViewports/setScissorRects and the uint viewport_array_index vertex output.

Sampler anisotropy uses the pinned clb197tex.h TEXSAMP0 MAX_ANISOTROPY
encodings (1, 2, 4, 6, 8, 10, 12, 16) when TEXSAMP1 MIN_FILTER is MIN_ANISO.
Apple MTLSamplerDescriptor.maxAnisotropy receives the decoded ratio, which is
part of the native argument-table cache identity. Point/linear modes retain
isotropic filtering. The split-TEXS pixel oracle exercises all eight ratios
with Metal API validation; it does not measure anisotropic quality on mip chains.

2D array textures use the same pinned block-linear TIC fields: TEXTURE_TYPE=5,
DEPTH_MINUS_ONE and normalized coordinates. Each single-mip array layer occupies
a separate block-linear 2D slice (block depth zero). Metal type2DArray views bind
at the shader table's array group, not the 2D group. A two-layer pixel oracle
selects the second distinct-colored layer with a hand-encoded TEXS array instruction.

Integer vertex input uses B197 SET_VERTEX_ATTRIBUTE_A NUM_UINT/NUM_SINT
and COMPONENT_BIT_WIDTHS_R16, with Apple's installed MTLVertexDescriptor.h
UShort/Short formats. The StageIn field uses uint4/int4 so ALD receives integer
register bits before guest conversion. Signed and unsigned scalar-fetch render
oracles exercise I2F with unsaturated pixel values; pipeline identity already
contains the attribute descriptors. SET_VERTEX_ATTRIBUTE_A begins at 0x1160.
The gpu_console capture now displays the sample's text and palette.
Rectangle-fill rasterization follows
[NV_fill_rectangle](https://registry.khronos.org/OpenGL/extensions/NV/NV_fill_rectangle.txt)
and pinned `clb197.h` SET_FILL_VIA_TRIANGLE/FILL_BBOX. A vertex wrapper evaluates
three original vertices and emits the projected bounding rectangle, preserving
winding, perspective/linear extrapolation and first/last flat provoking values.
Triangles crossing a clip plane are discarded. The implemented path supports
nonindexed triangles with procedural positions and constant-per-instance vertex
inputs; per-vertex fetch, special outputs and vertex global-memory effects are
rejected. FILL_ALL remains unsupported. Alpha testing uses the documented eight
comparison functions and discards failed fragments before color/depth updates.
The pixel oracle checks rectangle-only coverage, interpolated color, flat values,
and alpha discard with Metal validation enabled. The public deko console capture
was visually checked for complete glyph rectangles and palette colors.

Depth comparison sampling decodes DEPTH_COMPARE and DEPTH_COMPARE_FUNC from
the public TSC layout. TEX.DC retains its instruction-defined Shadow2D/array
resource type. Guest depth formats are converted to Depth32Float and uploaded
to private Metal depth textures; native samplers carry all eight comparisons.
Comparison state participates in sampler-table identity, and depth/color views
have separate texture identities. A Metal pixel oracle exercises all eight
functions with references below, equal to and above the stored depth (24 draws).

## 902D block-linear rectangle copies

The pinned NVIDIA cl902d.h above defines source/destination memory layout,
block height, pitch, dimensions and rectangle coordinates. Unity-derivative
RGBA8 SRCCOPY now supports both pitch and block-linear surfaces using the
existing texture GOB layout. Copies preserve outside pixels and padding and
snapshot aliased inputs before publishing any output. Storage is bounded to
64 MiB per surface. Scaling, fractional origins and filtering remain rejected;
this is a resolve prerequisite, not multisample resolve acceptance.

Validation: the copy-surface test exercises all 49 pitch/block-height pairs,
with disjoint and overlapping storage, partial rectangles crossing GOBs, and
invalid layouts/addresses without writes. The 13 GPU state tests also pass.

## Native multisample transfer foundation

Apple Metal Shading Language Specification (2026-06-04), sections on fragment
inputs/outputs and multisample texture reads:
https://developer.apple.com/metal/Metal-Shading-Language-Specification.pdf

The transfer helper uses per-sample fragment invocation, depth output and the
Metal 2.1 stencil reference output with replace operations. Its explicit 4x
expanded-row contract preserves each sample independently. It supports RGBA8,
BGRA8, Depth32Float and Depth32Float_Stencil8, rejects other layouts/formats and
bounds transfers to 64 MiB per primary plane. These tests establish the native
transfer contract; guest sample ordering and B197 integration remain separate.

The native test runs with Metal API validation, round-trips 560 independently
initialized samples across all four formats, verifies nonuniform stencil,
rejects short uploads without mutation, and compares two native color resolves
against the four-sample average while checking source sample preservation.

## B197 four-sample attachments

NVB197_SET_ANTI_ALIAS MODE_2X2 selects four-sample storage. The guest color/depth
extents are the expanded sample grid; Metal attachments use half those extents
in each dimension. Sample count participates in attachment and pipeline keys.
The packed SET_ANTI_ALIAS_SAMPLE_POSITIONS nibbles configure Metal sample
locations. Clears, loads, stores, guest writeback, eviction and re-import retain
individual samples. Unsupported sample modes/formats and mixed raster/storage
counts fail explicitly. Alpha-to-coverage remains unsupported; its inactive
dither footprint is retained without changing rendering.

A Metal validation test checks mixed edge coverage, sample-correlated depth,
stencil preservation, and re-import followed by depth-only clear. The full GPU
run passes 34 active render tests (five existing ignored), native cache checks,
13 state tests and two copy tests. Example06 advances from the MSAA setup stop
to the separate 902D compression/resolve path; scene acceptance is still pending.
