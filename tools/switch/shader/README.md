# soju-switch-shader

A dependency-free Rust crate for Soju's own GM20B (Tegra X1, Maxwell SM 5.3)
graphics shader path: SPH parser → instruction decoder → basic blocks and
structured control flow → Metal Shading Language.

**This is a foundation, not a complete translator for arbitrary commercial
shaders.** It recognizes 61 requested instruction classes in 145 encodings;
46 classes have partial lowering, including control flow handled by the IR.
Unsupported reachable operations/modes produce errors, not placeholder code.
All fixtures are original; no games, firmware, keys or commercial shaders are
used. [SOURCES.md](SOURCES.md) records every reference URL consulted.

## API

```rust,ignore
let shader = soju_switch_shader::translate(
    &binary, // 80-byte little-endian SPH, then control/instruction bundles
    &soju_switch_shader::msl::Options::default(),
)?;
// shader.source: MSL
// shader.entry_point: soju_vertex or soju_fragment
// shader.constant_buffers: required cbuf slots
```

The stages are independently accessible as `header::Header::parse`,
`decoder::decode`, `decoder::decode_instruction`, `ir::Program::build`, and
`msl::generate`. There is no CLI or runner integration yet. Translation performs
no file/network access or compiler invocation; compiler checks live in tests.

## Header and decoding

The 0x50-byte graphics header parser exposes stage IDs 0–5 (Vertex A/B,
tessellation control/evaluation, geometry, fragment), version fields, positive
and negative local allocations, warp-stack allocation, input/output maps,
fragment interpolation, color masks and depth-output enable. All 20 raw words
are preserved. The public header reference predates GM20B; complete extension
validation is not claimed. Special fragment color/point fields and unknown
fields remain in raw maps without invented attribute meanings.

Code offsets start at zero after the SPH. Each 32-byte bundle begins with one
control word followed by three instructions. A partial final bundle is accepted;
a lone control word, partial 64-bit word, or reserved high control bit is an
error. The decoder exposes stall/yield, read/write dependency barriers, wait
masks and register reuse. Signed branch offsets are relative to the next word
and include scheduling-word space.

Operands include registers, integer/float-bit immediates, cbuf addresses, memory
addresses, predicates and branch targets. Single-bit modifiers carry operand
associations. Multi-bit modes are retained as named subtable IDs, masks and raw
encoded bits; **these are not fully normalized semantic enums**. Not every
reserved mode is rejected by the decoder; the backend validates supported
modes before lowering. Unknown opcodes report the byte address and raw word.

| Family | Classes recognized and covered by reference-based classification tests |
| --- | --- |
| Float | FFMA FADD FMUL FMNMX FSET FSETP MUFU F2F F2I I2F I2I |
| Integer/logic | IADD IADD3 ISCADD IMAD XMAD ISET ISETP IMNMX LOP LOP3 SHL SHR BFE BFI POPC FLO SEL MOV PSETP P2R R2P |
| Memory | LD ST LDG STG LDS STS LDL STL LDC |
| Attributes | IPA ALD AST OUT |
| Textures | TEX TEXS TLD TLDS TLD4 TXQ |
| Control | BRA SSY SYNC PBK BRK EXIT KIL NOP DEPBAR BAR |

Applicable register, short/full immediate and cbuf forms from the pinned
reference are included; not every class has every form. Envytools' address-space
LD/ST spellings are normalized to ALD/AST, LDS/STS, LDL/STL and LDC. System-register
MOV/S2R, atomics, half/double arithmetic, indirect jumps/calls and other unlisted
classes are not recognized.

## Supported Metal subset

Every emitted instruction retains its execution predicate. R0–R254 are uint
locals with float bit casts; RZ reads zero and discards writes. P0–P6 are bools,
PT is true. Register/local/output initialization is deterministic zero, not a
claim about hardware reads before writes.

| Lowered classes | Accepted forms and limits |
| --- | --- |
| MOV | Register/cbuf/short/full immediate, component mask 0xf |
| FADD FMUL FFMA | f32 nearest rounding, applicable negate/abs/saturate and FTZ; no FMZ/scaling/condition-code writes |
| FMNMX | Predicate-selected f32 min/max, no FTZ |
| FSET FSETP ISET ISETP PSETP | Ordered/unordered f32 and signed/unsigned 32-bit comparisons; AND/OR/XOR predicate combinations; no carry or FTZ |
| MUFU | EX2/LG2/RCP/RSQ/SQRT using Metal functions; no COS/SIN/FP64 helpers |
| F2F F2I I2F I2I | 32-bit types; integral F2F/F2I rounding; no integer neg/abs or integer saturation |
| IADD IADD3 ISCADD IMAD | Wrapping 32-bit arithmetic, full-width operands and low products; no carry/saturation/high products/subword modes |
| XMAD | Unsigned low-16 × low-16 plus full-width addend |
| IMNMX SEL | Predicate-selected 32-bit operands, no extended min/max modes |
| LOP LOP3 | Basic binary logic and register-form ternary LUT; predicate destination must be PT; no extended modes |
| SHL SHR BFE BFI POPC FLO | Basic 32-bit operations, signed/unsigned SHR/BFE/FLO and shift wrapping; no reversal/carry/FLO.SH |
| LDC | Direct aligned nonnegative 32-bit cbuf access |
| LDL STL | Static aligned 32-bit accesses within SPH allocation, maximum 64 KiB |
| ALD AST | Vertex B direct 1–4-component accesses, RZ index registers, SPH-enabled addresses |
| IPA | Direct fragment PASS/CONSTANT inputs already interpolated by Metal; RZ arithmetic operands, PT sample predicate, center interpolation |
| TEX | Float 2D, component mask, implicit fragment LOD or level zero; bound/normalized bindless table indices |
| BRA SSY SYNC PBK BRK | Direct targets, condition 0xf, lexical forward selections/single-latch loops, consistent token stacks |
| EXIT KIL | Predicated termination; KIL requires fragment stage |
| NOP DEPBAR | Omitted after validation; lowered operations expose no Maxwell asynchronous dependency state |

**Decoded but not lowered:** P2R R2P LD ST LDG STG LDS STS OUT TEXS TLD TLDS TLD4
TXQ BAR. Unsupported modes of otherwise lowered classes also fail. Unreachable
instructions are decoded but do not require lowering.

Vertex B and fragment entry points use SPH-derived structs. Generic vertex
inputs 0–30 use `attribute(N)`; position uses `position`; generic varying
components use matching `user(locnN)` names. Fragment interpolation follows the
SPH. Special attributes, fragment depth, Vertex A/B composition, geometry and
tessellation are unsupported. Fragment colors use consecutive registers in
target/component-mask order; that ABI still needs independent hardware validation.

The structurizer rejects indirect/invalid targets, inconsistent/underflowed
stacks, crossed SSY/PBK tokens, loop-interior entries, multiple latches and
nonlexical exits. Some otherwise reducible graphs are deliberately rejected.
Limits: 65,536 instructions, 16 tokens and 64 nesting levels. Reachable code
fallthrough is an error. There is no state-machine fallback or iteration cap.

## Resource ABI and numerical contract

Cbuf slot N is `constant uint* cbN [[buffer(N)]]`. Callers supply sufficiently
large aligned storage. Slots above Metal's 0–30 binding range are rejected.
Guest GPU virtual addresses are not mapped.

TEX uses a Metal argument buffer at binding 30 by default, configurable through
`Options::texture_buffer`. It contains `texture_count` float 2D images at IDs
starting at 0, and the same number of samplers starting at `texture_count`.
Default count is 128; configurable count is 1–4096. Cbuf/table binding collisions
are errors; actual device/resource limits remain the caller's responsibility.

**Texture handles must already be normalized to texture/sampler-pair table
indices.** Bound TEX uses its 13-bit immediate; TEX.B uses its handle register.
Both arrays use the same index. Dynamic indices must be in range. Native packed
TIC/TSC descriptors and cbuf-based handle resolution are not implemented; this
is a provisional integration ABI, not transparent native-handle compatibility.

Compile with `-fno-fast-math -ffp-contract=off`. FADD/FMUL stay separate, FFMA
uses explicit `fma`, and FTZ/saturation have explicit helpers. This is not a
bit-exact IEEE/GM20B implementation: Metal denormals, NaN payloads, signed-zero
min/max ties, special functions, exceptional conversions and derivatives under
divergent control flow remain limitations. [SEMANTICS.md](SEMANTICS.md) details
the behavior and deviations. No GPU execution conformance is claimed.

## Verification

From this directory:

```sh
cargo test --offline
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

There are 27 tests: header/operand/scheduling tests, classification coverage for
all 61 classes, rejection tests, and eight hand-assembled programs translated
and compiled using:

```sh
xcrun -sdk macosx metal -std=metal3.0 -fno-fast-math -ffp-contract=off \
  -c target/metal-fixtures/passthrough_vertex.metal \
  -o target/metal-fixtures/passthrough_vertex.air
```

Fixtures cover position passthrough, texture × cbuf color, bindless level-zero
TEX, SSY/SYNC and PBK/BRK loops, predicated EXIT/KIL, arithmetic/conversions/local
memory, and comparisons/predicate logic. Generated `.metal`/`.air` evidence stays
in ignored `target/metal-fixtures`. Compilation skips only when `xcrun` itself
is absent; an installed but broken/missing Metal toolchain is a failure.
Compilation proves source acceptance, not rendering or numerical equivalence.

The declarative ISA facts come from pinned MIT envytools, with its notice in
[LICENSE.envytools](LICENSE.envytools). `scripts/import_isa.py` accepts only the
pinned file SHA-256, performs no network access, and is not required by the build.
The parser/structurizer/backend implementation and shader programs are original.

## Next steps

1. Establish hardware-backed tests for operand modes, exceptional arithmetic,
   interpolation/export registers and reconvergence/helper lanes.
2. Resolve native texture handles/descriptors, implement remaining texture and
   memory operations, and connect the resource ABI to the runner.
3. Add carry/condition codes, predicate transfers, missing types/modifiers and
   more general reducible-CFG structuring with uniformity analysis.
4. Add compute entry points, shared/global address mapping, barriers and atomics;
   compute requires its own launch metadata rather than this graphics SPH.
5. Emulate geometry/tessellation and Vertex A/B composition with explicit passes
   and intermediate buffers.
6. Design shader cache keys from exact header/code bytes, translator/ABI version,
   options, resource/vertex layouts, compiler settings and target GPU features.
   No shader cache or compiled-library persistence is implemented yet.
