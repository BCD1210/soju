# Backend contract and numerical limits

Compile generated MSL with `-fno-fast-math -ffp-contract=off`. The source also
turns contraction off. R0–R254 are uint locals; float operands use `as_type`,
RZ reads zero, and RZ writes are discarded. P0–P6 are bool locals; PT is true.
Registers, local memory and outputs start at zero for deterministic fixtures;
this does not establish hardware behavior for reads before writes.

FADD and FMUL are separate f32 operations; FFMA uses explicit `fma`. Only
round-to-nearest arithmetic is accepted. FTZ on FADD/FMUL/FFMA is implemented
with sign-preserving bit operations before and after the operation. Other
FTZ forms, FMZ, scale modes and carry/condition-code writes are rejected.
Saturation maps NaN to +0 and clamps to [0,1]. Ordered/unordered comparisons
check NaNs explicitly. Integer addition/multiplication wrap modulo 2^32;
shifts and bitfields guard against undefined shift counts.

This is not a bit-exact floating-point implementation. Metal devices may flush
subnormals even when Maxwell's FTZ bit is clear. NaN payloads, signaling NaNs,
min/max signed-zero ties, and special-function approximation errors are not
proven equivalent to GM20B. MUFU EX2/LG2/RCP/RSQ/SQRT use Metal functions; COS,
SIN and FP64 helper variants are rejected. F2I uses the requested integral
rounding, clamps overflow, and maps NaN to zero (PTX-style); GM20B exceptional
conversion results have not been validated on hardware. F2F/I2F/I2I currently
accept only 32-bit types. No GPU execution oracle or hardware conformance
claim is made by successful source compilation.

Control flow preserves per-invocation branches and termination. The supported
subset has lexical forward selections and single-latch loops with consistent
SSY/PBK stacks. It does not emulate warp execution masks, helper invocations,
or Maxwell reconvergence scheduling. Implicit texture derivatives in divergent
control flow are therefore not guaranteed equivalent. KIL emits Metal discard
and returns immediately. DEPBAR is elided because lowered operations expose no
Maxwell asynchronous work; BAR and shared memory require a future backend.

Vertex attribute addressing follows SPH byte addresses. Fragment outputs use
consecutive registers for enabled components in target/component order; this
ABI is an implementation assumption to validate against original shaders on
hardware. Vertex A/B chaining, special varyings and fragment depth are rejected.

The current TEX ABI requires the caller to normalize descriptors to one table
index per texture/sampler pair. Bound TEX's 13-bit immediate is that table
index; TEX.B reads the normalized index from its handle register. The host must
provide valid in-range indices and populate both argument arrays. Packed native
TIC/TSC handles and handle loads from a configured cbuf are not resolved here.
This is an explicit runner integration boundary, not transparent compatibility
with arbitrary native texture handles. Only float 2D TEX, implicit fragment LOD
or explicit level zero, is lowered. Texture reads preserve the component mask
and pack selected components into consecutive destination registers.

## Compute and execution oracle

`compute::Launch::parse` accepts the documented 256-byte NVIDIA Maxwell QMD
0.6/1.7 fields, preserving all raw words. The caller selects the program start,
resolves constant buffers, and dispatches exactly the parsed local dimensions;
those surrounding launch fields are not executed by this crate. Allocation is
limited to 48 KiB shared and 64 KiB per-thread local storage. S2R/MOV exposes
thread and CTA x/y/z IDs; other system registers are rejected.

LD/ST/LDG/STG accept aligned b32 accesses into one explicitly configured guest
address window. Extended addresses use a register pair. Invalid or unaligned
reads return zero and stores are dropped: this is a robust host boundary, not
Maxwell fault emulation. Generic LD/ST require PT and are global-only. Cache
policy modifiers other than the default remain rejected. LDS/STS use b32
threadgroup memory, which is uninitialized until the shader writes it.
BAR.SYNC accepts only barrier zero with a full-CTA count. Programs containing
barriers must have straight-line control flow with no conditional termination;
predicated arithmetic and disjoint memory operations remain allowed. This
conservative restriction prevents emitting a divergent Metal barrier.

P2R/R2P accept PR masks (P0–P6) and byte selectors; CC transfers are rejected.
P2R preserves unselected destination-source bits. Source values are captured
before predicate/register destinations are changed.

The Rust `reference` module interprets validated IR with wrapping integer
operations, separate f32 add/multiply and explicit fused multiply-add. It has a
bounded cooperative CTA scheduler for barrier tests. Callers must first validate
supported modes using translation. It is an independent implementation of the
backend contract, not a Maxwell hardware reference. Execution budgets are
oracle safeguards and do not affect generated MSL. The Swift test runner loads
an offline-compiled metallib, dispatches Metal, checks command completion, and
reads shared-buffer results. GPU tests skip only on non-macOS platforms.

FMNMX now explicitly orders −0 below +0 for minimum and +0 above −0 for maximum.
This fixes an observed CPU/Metal discrepancy; it is a PTX-style contract, not a
claim about undocumented GM20B tie behavior. Tests compare NaNs by classification,
not payload. Arithmetic-grid tests do not establish denormal preservation or
MUFU approximation bounds outside their tested inputs.
