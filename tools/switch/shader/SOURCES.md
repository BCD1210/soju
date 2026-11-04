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
