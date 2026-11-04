# Runner frame-rate investigation

Baseline: `304f2b3`, es2gears, three presentations with opt-in host instrumentation.
First presentation: 7470.121 ms. Subsequent intervals: 6853.324 / 6628.972 ms.
These measurements start at the guest loop, excluding launch/load.

| Inclusive stage | Calls | Total ms |
| --- | ---: | ---: |
| Guest hardware execution | 632 | 14.927 |
| Host memory rendezvous acquisition | 1569 | 0.044 |
| SVC dispatch | 246 | 169.496 |
| IPC dispatch (includes GPU stages below) | 183 | 20750.818 |
| GPFIFO/pushbuffer decode | 4 | 0.108 |
| Mapped-memory snapshot | 4 | 38.662 |
| MME interpretation | 5 | 0.006 |
| Shader discovery | 840 | 48.929 |
| Draw preparation (includes translation and resource capture) | 843 | 6061.927 |
| Submit and GPU completion | 843 | 1874.150 |
| Readback, file transfer and guest-format conversion | 843 | 12667.635 |
| Transaction write diff | 4 | 18.306 |

Nested and concurrent timings overlap; columns cannot be summed into wall time.
Presentation conversion was not instrumented in this first measurement. Counters
record durations and static stage names only, never guest memory contents.

The baseline issues 281 native submit/readback pairs per frame. Backend lifetime
is one IPC submission, discarding pipeline and native resource caches at its end.
Full mapping snapshots cost 38.7 ms over four submissions, whereas repeated
attachment preparation and readback cost seconds. These are separate problems.

Time Profiler recording: ignored `dist/frame-profile/time-before.trace` (23.187 s,
normal exit); exported stacks: `time-before-samples.xml` in that directory.
Use `xcrun xctrace record --no-prompt` for unattended recording; a launch without
that option stalled with the child stopped and was terminated, not timed.

`SOJU_PROFILE=1` enables cumulative counters and Instruments intervals. Headless
completion prints the counters. `--frame-times <path>` records presentation
intervals. Signposts use subsystem `org.soju.switch`, category PointsOfInterest.

## Submission batching

Persistent backend/worker state now survives IPC submissions. Compatible graphics
commands are prepared together. CPU engines, semaphore/completion operations,
attachment changes, overlapping constant writes and input hazards retain readback
boundaries. Textured draws conservatively synchronize; no general texture hazard
optimization is claimed. Captured attachment conversions are reused by identity,
view and generation within the batch. Native readback only visits dirty targets,
not every historical cached target.

Ten presentations: first 616.093 ms; following nine mean 61.200 ms (16.34 fps).
Totals: snapshot 73.765 ms, shader discovery 90.204 ms, prepare 178.139 ms,
submit/wait 342.436 ms (includes cold native startup), readback 193.501 ms,
commit diff 31.404 ms, display conversion 8.212 ms. Native executions fell from
281/frame to 11 across the whole ten-frame run. The 60 fps requirement remains unmet.

Validation: all ten existing scene tests passed; eleven NV Metal tests passed,
including a new clear/overlapping constant write/next-submission clear test.
The baseline Metal System Trace was saved under ignored `dist/frame-profile/`;
it ran alongside validation and is for attribution only, not wall-time comparison.

## Persistent memory generations

The runner retains its checked GPU memory image while mapping identities match.
Unchanged pages keep their generations; only changed pages are copied. Transaction
commit compares only ranges written after the snapshot revision. Contiguous guest
mappings can be borrowed during the host lease; fragmented mappings still use the
copy fallback. This is not yet Metal no-copy guest memory. Shader extent/decoded
instruction discovery is bounded and generation-checked, including after guest
code changes. Packed-target writeback copies contiguous 16-byte pieces.

The 120-frame measurement remains below 60 fps: 7.788 seconds overall. Per-frame
averages: snapshot 7.803 ms, shader discovery 0.272 ms, prepare 17.429 ms,
submit/wait 10.909 ms, readback 18.665 ms, commit 1.198 ms, display 1.003 ms.
The run includes cold initialization, so submit/wait is not a steady-state value.
Core library: 120 checks passed before adding the borrowing test; the new
contiguous/fragmented/permission boundary test also passed. GPU memory generation
checks passed. There is no claim that these changes alone satisfy throughput.

## In-process readback

Normal readback now returns owned byte spans through a synchronous native callback,
with every callback allocation scoped to the request. Spill files remain only for
cache-pressure eviction. Public fence waits still return diagnostics; the backend
consumes typed readback results. No normal frame readback files are written/read.

120-frame exploratory run: 6.304 seconds; readback total 1235.596 ms versus
2239.847 ms in the preceding run. Validation compiled alongside this exploratory
measurement; final timing must use an otherwise idle test session. All 28 GPU
render tests passed with Metal validation. This removes file I/O, not the remaining
attachment copies or the synchronous guest visibility boundary.

## Shared capture buffers

Vertex captures reuse bounded geometric ranges by address and generation within a
batch. They use owned 16 KiB-aligned, zero-padded storage; native vertex/index
buffers borrow this storage with Metal shared/no-copy buffers. Native requests
complete all command buffers before the batch owner releases those allocations.
Compute storage remains separately owned because it can outlive a request.
Guest arena mappings are borrowed as checked contiguous segments, avoiding a
whole-mapping copy fallback when physical pages are fragmented. Live guest memory
is still isolated from GPU captures; this is not asynchronous arena sharing.

Idle 120-frame run: 5.201 seconds overall. Stage totals: snapshot 328.768 ms,
prepare 1879.291 ms, submit/wait 926.043 ms, readback 1080.965 ms. This is still
below the requested sustained throughput. GPU tests passed with Metal validation;
121 core library checks passed. New tests cover aligned capture ownership/padding,
fragmented aliases, unmapped holes, permissions and unchanged range generations.

## Draw-range reuse

Validated non-indexed draw state can now be referenced with a different first
vertex/count when every captured vertex stream covers the new range. Topology,
instance, resource bindings, pipeline state and range generations must agree;
indexed and quad expansion stay on their existing paths. Native references retain
per-draw first/count and independently captured constant offsets.

120-frame run: 4.451 seconds overall, prepare 1156.137 ms. All 29 GPU render tests
passed with Metal validation, including a new varying-first/count pixel oracle
and an out-of-bounds rejection. Sustained 60 fps is still not established.
