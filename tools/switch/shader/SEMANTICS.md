# Execution contract and limits

This document distinguishes a testable backend contract from claims about
undocumented native GM20B behavior. The Rust interpreter is an independent
implementation of this contract, not NVIDIA hardware. Translate first to
validate instruction modes before running `reference::Machine`/`run_group`.

## Registers and floating point

R0–R254 contain uint bits; RZ reads zero and discards writes. P0–P6 are bools;
PT reads true and discards writes. Registers, local memory and stage outputs
start at zero for deterministic behavior. Hardware reads before writes are not
specified here. Shared memory is uninitialized until written by the program.

Compile with `-std=metal3.0 -fno-fast-math -ffp-contract=off`. FADD/FMUL are
separate f32 operations; FFMA uses explicit `fma`/Rust `mul_add`. Accepted
arithmetic rounding is nearest. Explicit FTZ flushes input and output subnormals
with sign-preserving bit operations. FMZ, scales, carry/CC writes and unsupported
rounding/type modes are errors. Saturation maps NaN to +0 and clamps to [0,1].
Comparisons explicitly distinguish ordered/unordered NaNs. Integer arithmetic
wraps modulo 2^32; shifts and bitfields guard zero widths and oversized counts.

FMNMX orders −0 below +0 for minimum and +0 above −0 for maximum, using explicit
helpers instead of Metal's device-dependent zero ties. This PTX-style behavior
fixed an observed GPU/reference mismatch; native GM20B ties remain unvalidated.
MUFU EX2/LG2/RCP/RSQ/SQRT use Metal functions; COS/SIN and FP64 helpers are rejected.
Only SQRT is represented in the current arithmetic grid. General approximation
bounds and bit-exact native MUFU equivalence are not established.

F2I rounds as requested, clamps overflow and maps NaN to zero. Rust's saturating
casts implement the reference result. This is a PTX-style contract; exceptional
native GM20B results remain unvalidated. F2F/I2F/I2I accept only 32-bit types.

**Observed intentional difference:** Metal may flush subnormal operands even
with Maxwell FTZ clear. A 32-case comparison probe on this Mac produced 12
results differing from the IEEE reference and matching an FTZ reference. Those
cases are reported separately from equivalence passes. Signaling NaNs, payload
propagation and general denormal preservation are not guaranteed. Equivalence
tests permit differing NaN payloads only when both results classify as NaN.

## Predicates, control and memory

P2R/R2P consume PR masks over P0–P6 and a byte selector; CC is rejected. P2R
preserves unselected source bits. Predicate instructions capture their sources
before writing destinations, including overlapping predicate registers.

The IR preserves per-invocation branches and termination for lexical forward
selections and single-latch loops. It does not emulate warp execution masks,
helper lanes or native reconvergence scheduling. Implicit derivatives under
divergence remain a limitation. KIL discards and returns in fragment shaders.
NOP/DEPBAR are validated and omitted because the backend exposes no Maxwell
asynchronous dependency state. The reference interpreter has instruction budgets
to stop runaway fixtures; generated MSL has no artificial loop cap.

LD/ST/LDG/STG accept b32 accesses within one configured guest address window.
Extended addresses use a register pair; ordinary addresses wrap at 32 bits.
Invalid/unaligned reads return zero and stores are dropped. This is a robust host
boundary, not Maxwell fault emulation. Generic LD/ST require PT and support
only global addresses; nondefault cache policies are rejected. LDL/STL require
static aligned addresses within positive/negative local allocations, with the
negative allocation used as the storage bias. LDC requires direct aligned cbuf
access. Cbuf sizes remain the caller's responsibility.

QMD parsing accepts the public Maxwell 0.6/1.7 256-byte layouts, exposes local
x/y/z size, shared/local allocation fields and preserves every raw word. The
caller resolves program start and cbuf addresses, and dispatches exactly the
parsed local dimensions. Threadgroup size is limited to 1–1024 threads, shared
storage to 48 KiB, local storage to 64 KiB. Actual device limits must also hold.
S2R/MOV supports thread and CTA x/y/z only. Compute textures are still rejected.

LDS/STS use aligned b32 threadgroup memory. BAR accepts unconditional
full-CTA SYNC barrier zero, with zero/implicit count. A program containing BAR
must have straight-line control flow and no conditional termination; predicated
arithmetic and disjoint memory operations are allowed. This conservative check
prevents divergent Metal barriers. `run_group` requires every lane to reach the
same barrier or terminate together. Neither execution path repairs data races.
The reduction fixture initializes every shared cell and synchronizes all lanes.

## Graphics and textures

ALD/AST use direct SPH-enabled vertex addresses. IPA supports PASS/CONSTANT inputs
already interpolated by Metal, with RZ arithmetic operands and center sampling.
Fragment colors consume consecutive registers in target/component-mask order;
this export ABI remains an integration assumption requiring native validation.
Special varyings, fragment depth, Vertex A/B chaining, geometry and tessellation
remain errors. OUT is decoded only: public opcode/mode names do not establish
its export-handle updates, allocation or geometry-stream behavior.

`Options::texture_bindings` maps bound immediates and bindless register values
to independent texture/sampler indices and types. The returned table is sorted
and deduplicated; conflicting mappings are rejected. The host resolves native
descriptor memory. Empty mappings retain the older paired-index contract, not
transparent packed-handle compatibility. Unknown explicit handles and out-of-range
indices return zero; bindings must fit the configured count.

Argument IDs are `type_group * texture_count + texture_index`. Groups 0–8 are
2D, 1D, 3D, cube, 1D array, 2D array, cube array, 2D shadow, 2D array shadow.
Samplers begin at ID `9 * texture_count`. Only used image groups are declared.
The buffer binding defaults to 30; callers must invalidate the earlier sampler
ABI. Hardware resource limits remain host responsibilities.

TEX supports declared float/depth types, implicit fragment LOD and level zero.
Array layers precede coordinates; comparison references follow them in the
coordinate tuple. Native tuple semantics beyond the tested 2D subset are not
hardware-validated. Metal 1D/1D-array images have one level: level-zero sampling
uses their ordinary sample overload; mipmapped native 1D resources are outside
this ABI. TEXS/TLDS support single/two-component 2D outputs; split three/four
component destinations are rejected. TLD/TLDS use level-zero integer reads;
TLD4 supports 2D component gather without offsets; TXQ supports bound 2D dimensions.
Other query modes, bindless TXQ, explicit LOD/bias, multisampling and packed
offsets remain errors. Selected components are packed into destination registers.

The CPU texture oracle covers nearest clamp/repeat 2D, level-zero reads, component
gather and dimension queries. Nine offscreen fragment fixtures compare 144
pixels, using original translated vertex code and nonuniform texture data.
Other declared texture types have compiler checks only. ALD/AST register results
are not captured directly; their generated vertex positions are raster-tested.

## Evidence boundary

There are 8,176 passing GPU/reference comparisons plus a separate 32-case
allowed-denormal-difference probe. Every partially lowered class is represented
in the GPU fixtures, but not every mode. The harness compiles Swift and generated
MSL at test time, loads metallibs, checks Metal command completion and reads back
results. Missing Metal on macOS fails; GPU tests skip on other platforms. No
native Maxwell hardware, commercial shaders or Nintendo data are involved.

Closed RRO/MUFU pairs are represented as complete sine, cosine or exp2 operations
in the structured program. They use host mathematical functions, not the hardware
range-reduction bit representation. Only adjacent pairs with equal predicates,
equal reduction/consumer destinations, an unmodified matching consumer, and no
branch/reconvergence target at the consumer are accepted. This preserves the
observable dataflow while avoiding an assumed intermediate encoding. Other RRO
uses fail. Native/reference tests cover 96 finite inputs with relative/absolute
2e-6 tolerance; this does not prove hardware approximation equivalence.

Unsigned IADD.CC records the 33rd sum bit and IADD.X consumes it; negated
carry operands remain rejected. F2I now supports unsigned 16-bit output with
clamping to [0,65535] after rounding, and explicit input FTZ. TEXS A2D implicit
sampling is translated with an integer array layer and split coordinates; this
has a Metal compilation check, not a native array-sampling equivalence test.
ISBERD's destination, address register, mode and width decode, but its input
buffer addressing and tessellation execution semantics remain unsupported.
