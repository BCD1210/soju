# Reference ledger

No emulator or Mesa/nouveau codegen sources are used. Fixtures are original,
hand-encoded programs, not extracted software. ISA bit positions and opcode
values are facts transcribed from the following allowed references.

- https://github.com/envytools/envytools/tree/f102b82381f3f11cee113d16374c87091db039d9
  Pinned envytools revision (MIT).
- https://api.github.com/repos/envytools/envytools/git/trees/master?recursive=1
  Used only to locate documentation and resolve the above revision.
- https://raw.githubusercontent.com/envytools/envytools/master/envydis/gm107.c
  Initial ISA lookup; subsequent work uses the pinned version below.
- https://raw.githubusercontent.com/envytools/envytools/f102b82381f3f11cee113d16374c87091db039d9/envydis/gm107.c
  GM107 instruction masks, operand fields, modifiers and scheduling layout.
- https://raw.githubusercontent.com/envytools/envytools/f102b82381f3f11cee113d16374c87091db039d9/rnndb/graph/gf100_shaders.xml
  Public SPH register description within envytools (not codegen). Common
  graphics header fields, VTG attribute/export maps, fragment interpolation
  maps and color/depth exports. The implementation preserves raw words since
  this older family description does not establish every GM20B extension.
- https://raw.githubusercontent.com/envytools/envytools/master/docs/hw/graph/fermi/cuda/shader-header.rst
  Attempted documentation URL; unavailable, no information taken from it.
- https://docs.nvidia.com/cuda/parallel-thread-execution/index.html
  NVIDIA public PTX reference: integer wraparound, predicates, floating-point
  comparisons, saturation and conversion semantics. PTX is not a GM20B encoding
  specification; it does not establish bit-exact SASS special-function behavior.
- https://developer.apple.com/metal/Metal-Shading-Language-Specification.pdf
  Apple public MSL specification URL (web fetch exceeded size limit). MSL syntax
  is validated with the locally installed Metal compiler.

The decoder's declarative encoding facts are extracted from the pinned MIT
source with `scripts/import_isa.py`. `LICENSE.envytools` retains its notice.
No external reference is required by the build or tests.

- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/master/classes/compute/clb1c0qmd.h
  NVIDIA public Maxwell QMD 0.6/1.7 definitions: shared-memory size bits
  544–561, version 576–583, CTA dimensions 592–639, local allocations
  1440–1463 and 1472–1495. Only these fields are interpreted.
- https://raw.githubusercontent.com/envytools/envytools/f102b82381f3f11cee113d16374c87091db039d9/rnndb/graph/gk104_compute.xml
  Cross-check of launch fields; the NVIDIA Maxwell header takes precedence.
- https://raw.githubusercontent.com/envytools/envytools/f102b82381f3f11cee113d16374c87091db039d9/docs/hw/graph/maxwell/compute.rst
  Examined; placeholder documentation, no semantic facts taken.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/master/classes/3d/clb197.h
  Examined public texture pool and bindless-cbuf configuration fields. These do
  not establish packed-handle decoding; no native descriptor layout is guessed.
- https://docs.nvidia.com/cuda/archive/12.8.1/cuda-binary-utilities/index.html
  Public Maxwell instruction overview; not sufficient to establish OUT stream
  allocation or destination-register semantics.

The Apple MSL PDF was subsequently downloaded and inspected locally for texture
sample/read/gather/query types, argument-buffer IDs, and threadgroup barriers.
All downloaded reference files remain under ignored `target/reference`.

- https://api.github.com/repos/envytools/envytools/git/trees/f102b82381f3f11cee113d16374c87091db039d9?recursive=1
  Located compute documentation within the already pinned allowed repository.

Downloaded public-reference SHA-256 fingerprints for this extension:

| File | SHA-256 |
| --- | --- |
| NVIDIA `clb1c0qmd.h` | `0041719fd1619035e58eb882e04196743dc1475af65147ff851fdf9f81dd672c` |
| NVIDIA `clb197.h` | `175c50f29798b39c6858cafe31497e9e9798c70ca068a64856719003b5d088d7` |
| Apple MSL PDF, dated 2026-06-04 | `41538b30d2f1140a5b2a0c84ce0a9f7b67bf0c707e224cfea0bfe5a44aa26cf5` |
