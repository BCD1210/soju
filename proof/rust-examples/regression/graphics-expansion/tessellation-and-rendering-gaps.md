# Remaining rendering contracts

These are implementation gaps, not accepted scenes. No unsupported instruction
or stage is translated into an empty function to raise the coverage count.

## Tessellation: one scene, two audited programs

The control and evaluation DKSH programs at offsets 410053 and 410821 in the
local deko examples now fully decode, including ISBERD. Translation still fails
at the stage contract. `shader/src/msl.rs` accepts Vertex B and Fragment;
`core/src/ipc/nv/metal/programs.rs` discovers only those two graphics stages.
ISBERD addressing and cross-control-point ALD/AST are not implemented.

The planned route is Metal tessellation: translate the control stage into a
compute function producing bounded per-patch factors and control-point records,
then translate evaluation into a post-tessellation vertex function. This needs
an explicit patch-buffer ABI, per-invocation indexing, synchronization, patch
primitive/partition/winding state, and resource ownership across the two passes.
The public ISBERD encoding alone does not establish its complete addressing
contract; supplying zero or a lane index would be a guess. Example05 also needs
the source's smoothed four-pixel wireframe treatment. Pixel acceptance must
check the source surface at fixed tessellation factors, silhouette, and line
coverage. None of this backend is implemented in this change.

Apple documents the compute-factor/tessellator/post-tessellation sequence:
https://developer.apple.com/library/archive/documentation/Miscellaneous/Conceptual/MetalProgrammingGuide/Tessellation/Tessellation.html

## MSAA and resolve: two scenes

Example06 and Example07 request multisampling before their draws. The runner
rejects the multisample state; its texture/attachment protocol still represents
single-sample storage. Supporting these scenes needs sample count in attachment
and pipeline identity, multisample color/depth allocation, defined load/store
lifetime, and an explicit resolved single-sample destination. Guest sample
layout and host resolve writeback must agree before subsequent copies or reads.
An edge-coverage oracle and a source-derived resolved-color oracle are required.
Apple's documented `type2DMultisample`, `sampleCount` and resolve attachment path:
https://developer.apple.com/documentation/metal/improving-edge-rendering-quality-with-multisample-antialiasing-msaa

## Viewport selection and multipass: one scene

Higher viewport/scissor setup now retains validated state; this is not viewport
array execution. Shader-selected viewport output is still unsupported and the
backend uses viewport zero. The current Example08 probe stops on an inactive/swapped vertex attribute
before later passes. It also needs multiple render targets and
subsequent sampled attachments. A vertex-amplification or per-viewport draw
implementation must preserve primitive routing and scissor/depth state, without
blindly duplicating primitives. Validate each viewport's source-colored region
and the final composition. Apple's viewport output contract:
https://developer.apple.com/documentation/metal/rendering-to-multiple-viewports-in-a-draw-command

## Compute integration: one scene

The isolated GPU crate has QMD parsing and compute dispatch tests; the runner's
B1C0 path retains setup and inline uploads but does not connect submitted QMDs
to that execution path. Example09 stops on a compute method before dispatch.
Integration needs bounded QMD/code discovery, actual constant/global binding
intervals, grid/local sizes, ordered writes, and visibility to the following
B197 draw. Static sinewave compute translation now succeeds; it does not prove
runner dispatch or the resulting vertex data. The acceptance oracle must use
the source sinewave at a fixed tick and verify dispatch-produced geometry.

## Remaining GL texture paths

The remaining texture scenes must resolve TIC texture type, sampler state,
array layers/mips and handles into the native resource protocol. Symbolic
texture bindings in the static audit test code generation only. A2D TEXS has
a Metal compilation fixture, while the runner still supplies D2 descriptors.
No array texture or mipmapped-sampler scene is accepted by that fixture.
