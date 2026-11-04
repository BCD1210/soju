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
