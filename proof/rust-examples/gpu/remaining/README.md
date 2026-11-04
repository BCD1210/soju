# Remaining graphics boundaries

Final probes use the signed runner at 8fb07bb, after the ten-scene acceptance
suite passes. These are failure probes, not accepted scenes. Their ordered
method tails and full-trace hashes are in
[`../../regression/graphics-expansion/blocker-method-tails.txt`](../../regression/graphics-expansion/blocker-method-tails.txt).

| Scene | Observed first boundary | Work still needed |
| --- | --- | --- |
| deko Example05 | B197 0x320=0x201 | Tessellation stage ABI, ISBERD addressing, patch execution, smoothed wide wireframe |
| deko Example06–07 | Multisample state gate | Native multisample attachments, sample-count pipeline state, resolve and guest writeback |
| deko Example08 | Inactive/swapped vertex attribute | Missing/default vertex-attribute path, subsequent multipass/MRT and viewport selection validation |
| deko Example09 | B1C0 0x2b4=0x02000a20 | Submitted QMD integration and ordered compute-to-graphics visibility |
| OpenGL textured_cube | TSC wrap/compare/aniso | Native sampler/mip state and source texture oracle |
| OpenGL gpu_console | TEXS array texture type mismatch | Typed TIC/native array resources and layer bindings |
| deko_console | NV ioctl 0xc0084105 | Requested channel operation, then graphics scene acceptance |

The corrected counted macro initialization now retains all viewport/scissor
array setup. This removes Example08's former 0xe14 setup gate, but does not
implement shader viewport selection. Its final probe now reaches vertex input
validation. No unclassified method is silently accepted to get past these stops.

OpenGL lenny, dynamic_resolution and SDL2 sdl2-simple are newly accepted;
es2gears remains accepted. See [`../accepted/README.md`](../accepted/README.md).
The installed corpus contains six OpenGL NROs and one SDL2 graphics NRO, with
no separate EGL NRO. dynamic_resolution is the instancing scene: it issues 64
instances and now has source-derived multi-instance pixel acceptance.

The static shader audit decodes all 521 occurrences. It translates 14/16
programs with symbolic typed resource mappings; the two tessellation stages
remain unsupported. The legacy resource audit is 11/16. Runtime resource binding
and native execution are separate from symbolic shader compilation.

Reproduce each failure with `scripts/probe-switch-gpu.py <signed-runner>
--scene <name> --output <directory>`. Probes request 100,000 frames with a 30-second
budget; menu captures and timeouts are never accepted images. See the
[backend gap document](../../regression/graphics-expansion/tessellation-and-rendering-gaps.md)
for concrete implementation contracts and public references.
