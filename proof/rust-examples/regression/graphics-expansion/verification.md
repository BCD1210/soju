# Verification

Final native acceptance: 10 passed, zero failures. Three newly accepted scenes:
SDL2 rectangles, OpenGL lenny, and OpenGL dynamic_resolution. Capture hashes are
unchanged between individual acceptance and the final complete suite. Tests use
a signed runner, synchronous Metal pipelines, and fixed capture tick zero.

- Core library: 113 passed.
- Shader: 57 passed, including native Metal arithmetic and compilation tests.
- GPU: 40 passed; five existing benchmarks/soaks ignored.
- All-target Clippy with warnings denied: core desktop, shader, GPU passed.
- Rustfmt checks and `git diff --check`: passed.
- Full test output is in this directory; the input binaries stay outside git.

The complete suite caught a regression that individual new-scene tests missed:
changing the initial fragment group caused deko Example04 to fail with
`unbound texture handle cbuf`. Trace analysis against the public driver's
counted register fill showed only the first of five initialization writes.
A focused five-write test exposed the delayed-exit loop bug. Correct execution
then exposed a missing context completion poll (`MME output limit`) and higher
viewport/scissor state that the prematurely stopped loop had hidden. The final
implementation validates the known context setup requests, returns completion,
retains the unused array state, and passes all ten pixel tests. Unknown context
requests, reversed scissor bounds and unsupported shader viewport output remain
rejected. No sampling tolerance was increased to repair this regression.

The dynamic mesh oracle initially had one mismatch adjacent to another
instance's silhouette. Its exclusion rule was extended by a two-pixel guard
around independently projected occluder boundaries; it never reads image colors
to choose samples. The same RGB tolerance two remains. The final check covers
74,707 surface pixels from 41 visible instances and 1,652,199 background pixels.
The single-mesh test checks 5,720 surface and 881,588 background pixels. SDL checks
all 2,073,600 RGBA pixels exactly.

Static audit before/after uses the same read-only audit program and settings
against the same 16 embedded DKSH programs at e7f5db4 and this continuation:
legacy resources 514/521 decoded and 8/16 translated -> 521/521 and 11/16;
symbolic texture resources 514/521 and 9/16 -> 521/521 and 14/16. The symbolic
mode gives handles compact typed slots; it is not native resource execution.
Two tessellation stage programs remain untranslatable. The runtime audit prints hashes,
counts and failures without extracting shader blobs. The static audit
identifies programs by container offset rather than hash.

The uninstrumented es2gears rerun also crossed the 15-second limit before the
cache follow-up. Program registration was discarding all shader/variant caches
on every draw, including unchanged registrations. Identical extent/options now
preserve those caches; changes still invalidate them, and the existing memory
generation identity protects modified code. A native three-pass regression
checks repeated registration and then changed shader bytes. The final timed
corpus is uninstrumented; the separate audited corpus retains its original
39/19/1 result.

Final uninstrumented Metal rerun after 8fb07bb: 39 frames / 19 stops / 1 exit,
versus e7f5db4's 37/21/1. New frame rows are dynamic_resolution, lenny and
sdl2-simple. es2gears moves from frame to timeout at the unchanged 15-second
budget, although its independent pixel test passes (15.647192 seconds to first
frame in the final fixed-pose acceptance run). The cache preservation change
does not resolve this timing gap. Default backend remains 34/24/1.
