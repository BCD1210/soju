# Mixed-state submission throughput

Public `Engine`, `Snapshot`, `Backend`, and `Fence` signatures are unchanged.
The runner can merge these commits without changing its integration. No registry
dependencies were added and the shader/texture crates are unchanged.

## Implementation

1. Rust constructs C-layout command descriptors with owned typed spans. An
   eight-slot single-producer/single-consumer atomic ring transfers batch ownership
   to a native worker thread. Release/acquire publication, bounded backpressure,
   and queue draining preserve submission order. Native calls borrow allocations
   until GPU completion. No command serialization, JSON, upload manifests, pipes,
   or resource-file writes remain in submission. Fence diagnostics remain strings
   for compatibility; explicit readback still uses files.
2. A bounded, batch-local cache holds 64 validated draw variants. Reuse checks
   exact registers, bindings, action, memory identity, captured resource-range
   generations, and attachment-view context. Constant data is independently
   versioned. Register-map mutation remains supported. The worker tracks pipeline,
   depth state, viewport, bias, scissor, raster, blend color, stencil reference,
   vertex buffers, constants, and texture tables independently. Exact register-group
   transition masks avoid repeated comparisons. New encoders invalidate bindings.
3. Texture/sampler handles are resolved once into persistent tier-2 argument-buffer
   heap slots. A switch writes the table's offset through Metal's buffer-offset
   setter. Residency is declared once per texture per encoder. This is table-slot
   addressing, not a new shader ABI or unrestricted guest bindless addressing.
4. Cbuf capture uses 256-byte-aligned upload offsets, a range-generation cache, and
   fixed per-slot offset arrays in reference commands. Unchanged slots keep their
   offsets; changed bytes append to a contiguous heap. Capacity grows geometrically
   (at least 1 MiB on the first capture), without an allocation per updated cbuf.
   One batch copy feeds one of three reusable Metal uniform buffers. The worker
   completes a submission before reusing a slot; three submissions do not execute
   concurrently.
5. Render-pass descriptors are cached by ordered target set and depth generation.
   Resolved per-command pass state avoids rebuilding descriptors for references.
   Color-only draws to disjoint target sets may be grouped, preserving all draws
   and their order within each set. Clears, queries, any depth attachment, shared
   MRT members and attachment-generation changes delimit scheduling spans.
   Shader storage writes are not admitted by this graphics backend. Every draw
   remains a real Metal draw; no final-image substitution is used.

The Swift library is compiled once per process with optimized debug information.
Each backend gets a separate native instance. Struct strides are checked before
calling the native ABI. The library stays loaded because registered Swift metadata
must outlive all instances.

## Default benchmark

Apple M4 Pro, release mode, validation disabled for timing. Each warmed mix issues
2,000 six-vertex draws at 32×32; every draw changes the named state. The unchanged
fixture includes method decoding, snapshots, preparation, submission and GPU
completion. Compilation/warmup and final readback are excluded. Final pixels are
checked. Fresh baseline: `ed0f136`; times vary with system load.

| Mix | Before draws/s | After draws/s | Requirement |
| --- | ---: | ---: | ---: |
| Same state | 355,571 | 626,697 | — |
| Texture pool switch | 10,465 | 370,751 | 300,000 |
| Cbuf update | 14,751 | 410,534 | 300,000 |
| Render-target switch | 13,107 | 483,252 | 100,000 |

The earlier supplied figures (470,473 / 13,757 / 16,175 / 10,914) were from a
different run of the same baseline. The table uses this session's fresh baseline.
Evidence: `target/gpu4-before.log`, `target/gpu4-final-exact-benchmark.log`.

| After, 2,000 draws | Decode/snapshot ms | Prepare ms | Submit ms | Native encoding ms | GPU completion wait ms | Encoders |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Same | 0.901 | 0.365 | 0.080 | 0.306 | 1.014 | 1 |
| Textures | 1.098 | 0.606 | 0.070 | 2.097 | 0.931 | 1 |
| Constants | 0.903 | 0.735 | 0.059 | 1.638 | 0.896 | 1 |
| Targets | 0.989 | 0.383 | 0.056 | 1.656 | 0.615 | 2 |

Columns are measured phases, not a complete partition of wall time. Native
encoding includes scheduling, state resolution and Metal calls; completion wait
includes commit and synchronization, not GPU-only execution time. Texture and
constant mixes each use five total pipeline/buffer binds and 1,999 offset writes.

Before safe target scheduling, descriptor caching alone reached 38,053 target
switches/s: encoding 18.241 ms, completion wait 32.403 ms, 2,000 encoders. The
higher final target result depends on independent color attachments. Shared-depth
or other ordering-sensitive target switches still need distinct passes and do
not inherit this throughput result.

## Instruments evidence

Time Profiler traces and XML exports are retained under ignored `target/`.
`gpu4-before-focused.trace` and `gpu4-before-symbols.trace` profile the original
Rust process. `gpu4-before.trace` additionally captures the old native processes;
its exported summary filters to this task's benchmark workers. The all-process
recording logged unrelated simulator image-map warnings; the focused recordings
completed successfully.

The matched 20,000-draw recordings (`gpu4-before-20k.trace` and
`gpu4-after-20k.trace`) both complete and pass their image checks. In the baseline
front-end sample, 1,215 of 1,332 sampled CPU milliseconds included
`Backend::prepare_state`; formatting included 262 ms and hashing 136 ms.
The updated recording has 116 selected CPU samples, including the now in-process
native thread: preparation appears in 23 ms, method/snapshot processing in 21 ms,
`Worker.run` in 43 ms, memory copies in 35 ms, and pass scheduling in 6 ms.
The baseline focused trace excludes the old child worker, so its CPU total is
not an end-to-end total and cannot be directly subtracted from the new total.
The original native-worker sample had 206 CPU samples: 61 included Swift dynamic
casts, 41 JSON reader work, and 15 JSON writer work for argument-table identities.
These are inclusive stack counts and overlap; they must not be added together.

The 100,000-draw updated profile (`gpu4-after-profile.trace`) contains 532 selected
CPU samples: 320 on the test/front-end thread and 212 in the native call. Inclusive
stacks include 139 ms in method/snapshot processing, 115 ms in preparation, 44 ms
in BTreeMap snapshot cloning, 152 ms in memory copies, 36 ms in Metal draw calls,
and 13 ms in pass scheduling. JSON is absent from the submission implementation.
Snapshot allocation and copying fixed-size native descriptors remain measurable
costs; the borrowed `method_into` API can avoid caller snapshot allocation.
The longer profile is for attribution, not the default benchmark table.

An exploratory 100,000-draw baseline run exhausted Metal shared command storage
on the target-switch mix (`Failed to allocate IOGPUDeviceShmem`). Its target result
is not reported as a performance result. The complete matched comparison uses
20,000 draws; the original default 2,000-draw fixture passes unchanged.

## Reproduction and verification

From `tools/switch/gpu`:

```sh
cargo test --offline --release --test render throughput_state_mixes -- --ignored --nocapture
MTL_DEBUG_LAYER=1 cargo test --offline
MTL_DEBUG_LAYER=1 cargo test --offline --release --test render -- --ignored --nocapture
cargo clippy --offline --all-targets -- -D warnings
CARGO_PROFILE_RELEASE_DEBUG=1 cargo test --offline --release --test render --no-run
# Use the executable path printed by the last command:
xcrun xctrace record --template 'Time Profiler' \
  --env SOJU_GPU_BENCH_DRAWS=20000 --output target/profile.trace \
  --target-stdout target/profile.log \
  --launch -- target/release/deps/render-<hash> throughput_state_mixes --ignored --nocapture
```

The optional draw-count environment variable changes only the ignored benchmark;
its default remains 2,000 and the count must be even and at least two.

The 22 existing GPU tests and five new tests pass with Metal validation (27 total).
Both ignored benchmarks also pass with validation. Existing image checks remain;
the new regressions add 7,168 pixels covering 24 queued batches, captured cbuf
lifetimes, cache invalidation after vertex/public-register changes, independent
uniform-slot updates, per-target blend ordering and shared-depth ordering.
Shader regression: 44 tests. Texture regression: 7 tests. All three crates pass
offline Clippy with warnings denied. GPU formatting passes.

## Limits

First-seen state variants still take the complete validation/resource-resolution
path; the cache is bounded to 64 variants per batch. Native dirty groups cover
the implemented states, not unimplemented Maxwell registers. Persistent pipeline,
texture, argument-heap and pass caches have no eviction. Heap growth allocates
occasionally; steady-state constant updates do not allocate individual buffers.
The ring avoids command-transport locks; fence notification and Metal internals
are not claimed to be lock-free. Submission completion remains serialized, and
readback remains explicit and file-based. No retail game throughput or new shader
coverage is claimed.
