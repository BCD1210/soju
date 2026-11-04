# Graphics expansion from e7f5db4

Three new scenes are accepted with source-derived pixel tests and committed
captures: SDL2 sdl2-simple, OpenGL lenny, and OpenGL dynamic_resolution. The full
ten-scene suite passes. No commercial-title compatibility is established.

## Local step commits

| Commit | Change |
| --- | --- |
| 9e04e8a | Closed RRO/MUFU range-reduction pairs |
| e5fea9a | Reconverging shader diamonds and comparison FTZ |
| 6c976e9 | Integer instance IDs and native separate-instance draws |
| fa81367 | Z16 mapping retention and exact SDL rectangle acceptance |
| ef30f43 | Zero-wins FMUL.FMZ and F2F.FTZ |
| d781448 | Bounded MME variable bitfield shifts |
| dd076ca | IADD carry, F2I FTZ/U16, ISBERD decoding, TEXS A2D compilation, typed audit |
| 49eabad | Initial stage groups and two lit mesh scene acceptances |
| 5f8e382 | Opt-in runtime shader census without extracted blobs |
| a50be89 | Complete MME loops, bounded context completion, retained array setup |
| 8fb07bb | Preserve caches on identical registrations; native changed-byte regression |

All commits remain local. The complete-suite regression and its fixes are
recorded in [verification.md](verification.md), alongside test totals and oracle
limitations. Captures and pixel rules are in
[accepted scenes](../../gpu/accepted/README.md). Input/source identities are in
[source-hashes.txt](source-hashes.txt); the signed runner identity is in
[runner-hash.txt](runner-hash.txt).

## Reproduce

```sh
bash scripts/test-switch-gpu-accepted.sh
cargo test --offline --manifest-path tools/switch/core/Cargo.toml --features desktop --lib
cargo test --offline --manifest-path tools/switch/shader/Cargo.toml
cargo test --offline --manifest-path tools/switch/gpu/Cargo.toml
cargo run --offline --manifest-path tools/switch/shader/Cargo.toml --example corpus_audit -- "$HOME/.local/share/soju/switch/examples"
cargo run --offline --manifest-path tools/switch/shader/Cargo.toml --example corpus_audit -- "$HOME/.local/share/soju/switch/examples" --symbolic-textures
env -u SOJU_SWITCH_AUDIT_SHADERS -u SOJU_SWITCH_GPU SOJU_SWITCH_PROOF="$PWD/dist/corpus-default" bash scripts/test-switch-examples.sh
env -u SOJU_SWITCH_AUDIT_SHADERS SOJU_SWITCH_GPU=metal SOJU_GPU_ASYNC_PIPELINES=0 SOJU_SWITCH_PROOF="$PWD/dist/corpus-metal" bash scripts/test-switch-examples.sh
```

For the before audit, an isolated e7f5db4 archive uses the same audit example
(with its symbolic flag) against the old shader crate. No shader blobs are
extracted. The default resource mode is 8/16 -> 11/16; symbolic typed slots are
9/16 -> 14/16. Both after modes decode 521/521 occurrences, versus 514/521.
**16/16 is not achieved:** the two tessellation stage programs remain unsupported.

The runtime census is enabled separately with `SOJU_SWITCH_AUDIT_SHADERS=1`.
It records the shaders reaching bounded graphics registration with actual cbuf
lengths and handle mappings. The 59-example instrumented run plus selected-scene
probes reached 16 distinct blobs (705 decoded instruction occurrences); 15 translated, one failed on TEXS array resource
type. [runtime-shaders.txt](runtime-shaders.txt) and
[runtime-shader-scenes.txt](runtime-shader-scenes.txt) preserve their identities
and provenance. This is not a census of shader stages hidden behind earlier
method gates. It currently rechecks repeated programs, so instrumented runs are
kept separate from uninstrumented timing comparisons.

The default corpus is 34 frames / 24 stops / 1 clean exit, unchanged. The
matching uninstrumented Metal comparison is **37/21/1 -> 39/19/1**: three new
frames, with es2gears crossing the 15-second cutoff. Its fixed-pose pixel test
continues to pass. The cache follow-up did not restore that timed frame. See the
adjacent corpus summaries for the matching explicit-Metal baseline, final run,
and separate instrumented run. Each uses the same 59 NROs, first presentation,
and 15-second per-example budget. A console/menu presentation is only a boot
metric, not pixel acceptance. The baseline Metal runner was rebuilt from an
isolated e7f5db4 archive.

[blockers.txt](blockers.txt) gives the final failure for every remaining requested
scene; [blocker-method-tails.txt](blocker-method-tails.txt) preserves ordered
method evidence. The [backend gap document](tessellation-and-rendering-gaps.md)
explains the tessellation, multisample, viewport/multipass, compute and texture
contracts still missing, with public references. These are concrete remaining
implementation gaps; no empty stage or opcode no-op is counted as translation.

Final failure probes were repeated at 8fb07bb with identical boundaries and no
new shader identities. The instrumented 59-example census itself is from a50be89;
the later change only preserves identical-registration caches.
