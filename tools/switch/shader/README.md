# soju-switch-shader

Dependency-free, clean-room GM20B/Maxwell shader translation:
graphics SPH or compute QMD → decoder → structured IR → Metal source.

**This does not translate arbitrary commercial shaders.** There are 61 recognized
classes in 146 encodings (including S2R normalized to MOV). **60 classes have
partial lowering; 14 of the previous 15 decode-only classes are now lowered.**
`OUT` remains unsupported: correct geometry export-handle/stream behavior has
not been established from the permitted sources. Unsupported reachable modes
return errors. No games, firmware, keys or extracted shaders are used.
[SOURCES.md](SOURCES.md) records the reference ledger.

## APIs

```rust,ignore
let graphics = soju_switch_shader::translate(&sph_and_code, &options)?;
let launch = soju_switch_shader::compute::Launch::parse(&qmd)?;
let compute = soju_switch_shader::compute::translate(&code, &launch, &options)?;
// source, entry_point, constant_buffers, uses_texture_table, texture_bindings
```

Graphics supports standalone Vertex B and Fragment. Compute takes scheduling
bundles without a graphics header. QMD 0.6/1.7 parsing exposes local x/y/z size,
shared bytes, positive/negative local allocations and all 64 raw words. The host
resolves program start/cbuf addresses and dispatches the parsed local size.
The adjacent `../gpu` crate now supplies a separate B1C0 engine, bounded QMD
resource resolution, Metal dispatch and deferred global-buffer readback; see its
README for the standalone-grid subset. This shader crate remains I/O-free.
Translation itself invokes no compiler and performs no I/O.

The graphics parser preserves all 20 SPH words, stage/version/allocation fields,
attribute maps, fragment interpolation and color/depth flags. Code bundles have
one scheduling word followed by up to three instructions. The decoder retains
operand associations and raw mode fields. Recognizing an encoding does not mean
all its modes are supported. Structuring accepts lexical forward selections and
single-latch loops with consistent SSY/PBK stacks; indirect targets, crossed
stacks, nonlexical exits and irreducible flow are rejected.

## Lowered subset

| Classes | Supported subset |
| --- | --- |
| MOV | Register/immediate/cbuf; S2R thread and CTA x/y/z IDs in compute |
| FADD FMUL FFMA | f32 nearest rounding, supported abs/neg/sat and explicit FTZ |
| FMNMX FSET FSETP | f32 min/max and ordered/unordered comparisons |
| MUFU | EX2/LG2/RCP/RSQ/SQRT; approximations are not Maxwell bit-exact |
| F2F F2I I2F I2I | 32-bit conversions; supported rounding only |
| IADD IADD3 ISCADD IMAD XMAD | Wrapping 32-bit arithmetic; XMAD low unsigned 16-bit products |
| ISET ISETP IMNMX SEL PSETP | Comparisons, selections and predicate combinations |
| LOP LOP3 SHL SHR BFE BFI POPC FLO | Basic 32-bit forms; no carry/reversal/extended modes |
| P2R R2P | PR mask transfers, all four byte selectors; CC rejected |
| LD ST LDG STG | b32 accesses into one configured address window, optional extended register-pair addresses; generic LD/ST global-only with PT |
| LDS STS BAR | b32 threadgroup storage and unconditional full-CTA BAR.SYNC 0; conservative convergence validation |
| LDL STL LDC | Static aligned b32 local/cbuf accesses |
| ALD AST IPA | SPH-enabled direct vertex attributes and fragment PASS/CONSTANT inputs |
| TEX | Typed float/depth resources, implicit fragment LOD or level zero |
| TEXS TLDS | 2D, single/two-component output masks; TEXS implicit/zero LOD, TLDS zero LOD |
| TLD TLD4 TXQ | Level-zero integer reads; 2D component gather without offsets; bound 2D dimension queries |
| BRA SSY SYNC PBK BRK EXIT KIL | Structured per-invocation flow; KIL is fragment-only |
| NOP DEPBAR | Validated and elided |

`OUT` is decoded but not lowered. Geometry/tessellation, Vertex A/B composition,
fragment depth, atomics, half/double arithmetic, carry flags, general CFGs and
many operand modes remain unsupported. Limits include 65,536 instructions,
16 control tokens, 64 nesting levels, 64 KiB local and 48 KiB shared memory.
Resources are discovered from reachable IR, excluding constant-false instructions.

## Resource ABI

- Cbuf slot N uses `constant uint* cbN [[buffer(N)]]`; slots must fit 0–30.
- `Options::global_memory` describes a buffer binding, guest base address and
  byte size. Loads outside the aligned window return zero; stores are dropped.
  This is robust host behavior, not guest fault emulation.
- `Options::texture_bindings` maps `Bound(immediate)` / `Bindless(register_value)`
  to separate texture and sampler indices and a `TextureType`. Conflicts are
  errors; output metadata is sorted and deduplicated. The host resolves native
  descriptors. Empty mappings preserve the legacy paired-index ABI.
- Texture groups are 2D, 1D, 3D, cube, 1D array, 2D array, cube array, 2D shadow,
  2D array shadow, numbered 0–8. Argument ID is `group * texture_count + index`;
  sampler IDs start at `9 * texture_count`. Only used image groups are declared.
  The argument-buffer binding defaults to 30. **This changes the old sampler ABI.**
- Texture count is 1–4096; actual device limits are the caller's responsibility.
  Unknown explicit handles return zero. Binding collisions are errors. Metal
  1D resources are single-level; mipmapped native 1D textures need another ABI.

Compile with `-std=metal3.0 -fno-fast-math -ffp-contract=off`.
[SEMANTICS.md](SEMANTICS.md) defines numerical behavior, address bounds,
barrier restrictions, initialization and intentional differences.

## Reference execution and verification

`reference::Machine` independently interprets validated IR; `run_group` schedules
lanes cooperatively at barriers. Integer operations are bit-exact for the stated
contract. Floating operations use Rust f32, separate add/multiply and fused FMA.
The oracle is **not NVIDIA hardware** and does not establish native SASS behavior
where the public documentation is incomplete.

The tests compile Swift harnesses with `xcrun swiftc`, compile generated MSL to
metallibs, dispatch real Metal kernels or render offscreen, check command status,
and compare readback. There are **43 Rust tests** on this Mac:

| Executed GPU fixtures | Compared cases |
| --- | ---: |
| 30 arithmetic programs × 256 input combinations | 7,680 |
| Shared reduction/predicate transfers, 32 lanes; independent sum = 528 | 32 |
| Five memory/predicate/scheduling programs × 32 lanes | 160 |
| Reconvergence/break loop, 32 inputs | 32 |
| Three explicit FTZ programs × 32 inputs | 96 |
| Negative local-address bias, 32 inputs | 32 |
| Nine fragment programs × 16 pixels, including two textures and predicated discard | 144 |
| **Total GPU/reference comparisons** | **8,176** |

A separate 32-case denormal probe is **not counted as equivalence**: 12 results
on this Mac differ from IEEE and match the documented FTZ alternative. NaN
payloads are ignored when both results are NaN; integer results and tested
signed-zero ties are compared exactly. Arithmetic coverage includes edge values,
overflow, shift/bitfield boundaries, infinities and NaNs.

All 60 lowered classes occur across the executed fixtures, including the five
structurized control classes. ALD/AST are exercised by the translated vertex
shader's rasterization; their register results are not directly read back.
Non-2D texture types have compilation checks only (18 implicit/level-zero type
variants). The eight original graphics compilation fixtures are also retained.
No claim of all-mode coverage, native Maxwell equivalence or real-game coverage
is made. GPU execution tests skip on non-macOS; missing/broken Metal on macOS is
a failure. Generated evidence stays under ignored `target/`.

```sh
cargo test --offline
cargo clippy --offline --all-targets -- -D warnings
cargo fmt --check
# Show individual GPU case counts:
cargo test --offline --test equivalence --test textures -- --nocapture
```

## Source cache

`cache::Key::for_shader` computes a stable 128-bit, length-framed multiply/XOR
hash over exact code/scheduling bytes, exact SPH/QMD bytes, translator ABI,
options, normalized mappings and caller-supplied canonical render state. Include
external vertex/resource layouts, compiler identity and target features in that
state. `Key::new` is the lower-level explicitly framed hash. Neither is a
cryptographic authentication mechanism.

`cache::encode`/`decode` implement format version 1: 8-byte magic, u32 version,
16-byte key, u64 source length, 16-byte payload checksum, UTF-8 MSL. Integers are
little-endian; source is limited to 16 MiB. Wrong keys/versions, corruption,
truncation and trailing bytes are rejected. Callers own file paths and atomic
publication. No compiled binaries or cache files are committed.

## Remaining work

1. Establish OUT's geometry stream/handle semantics using allowed evidence,
   then implement geometry export passes and tests.
2. Resolve native texture/cbuf descriptors in the runner; extend texture masks,
   query modes, explicit LOD/offsets and numerical tests beyond nearest 2D.
3. Validate undocumented Maxwell floating-point, interpolation/export and
   reconvergence behavior against lawful original hardware tests.
4. Add missing arithmetic modes/types, atomics, memory widths, general CFG and
   barrier uniformity analysis; integrate launch and resource metadata with Soju.

ISA facts are derived from pinned MIT envytools, with its notice in
[LICENSE.envytools](LICENSE.envytools). Importing the declarative table is an
optional offline step guarded by the pinned file SHA-256. Implementation and
hand-assembled fixtures are original.

Vertex output maps now include point size (0x6c) and eight clip distances
(0x2c0–0x2dc), from the pinned envytools SPH source. The GPU backend supplies
clip enable and raster point-size state; its image suite exercises a clip plane.
