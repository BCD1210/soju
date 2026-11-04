# Shader-selected viewport arrays

Original method streams and shaders select all sixteen viewport/scissor slots in
one draw. Every pixel in the 32x32 output is checked. Alternating negative X scales
exercise per-viewport position correction. A later draw changes viewport 15's
transform, scale sign and scissor and checks the changed output. Forcing the emitted
viewport index to zero fails the pixel oracle (`viewports-negative.txt`); restoring
the implementation passes the full render suite (28 tests, five existing ignored
long-running tests, `render.txt`).

The vertex output is an integer Metal viewport_array_index, preserving raw guest
register bits. Native state binds sixteen independent viewport/scissor entries;
state caching tracks all entries. Pipeline identity includes every scale sign.
Guest submission intersects all sixteen clip rectangles with scissors, preserves
those fields into the backend and converts each viewport's Y orientation.
Geometry-stage emission, viewport coordinate swizzles beyond the supported default,
and non-default depth clip ranges are not added by this change. Valid selected
viewport indices are 0 through 15.

The independent corpus shader audit remains 14/16 translated and 521/521 decoded.
Tessellation initialization/evaluation still need stage-specific execution and
attribute ABIs. This is not a tessellation pass.
