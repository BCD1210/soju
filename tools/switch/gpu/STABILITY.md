# First-seen variants and long-session stability

Verified on Apple M4 Pro, macOS, release benchmarks. Public Rust signatures and
native command ABI are unchanged. No dependencies were added. The shader and
texture crates were not modified. These are headless synthetic measurements,
not game compatibility or a guarantee of stutter-free presentation.

## First-seen frame submissions

`first_seen_shader_frames` runs 300 frames, changes the generated vertex MSL point
size every ten frames (30 distinct pipeline variants), and sleeps 16 ms between
frames. Timings include preparation, submission and GPU completion; pacing,
worker-library startup and final readback are excluded. The final comparison
disables archive persistence to isolate asynchronous compilation. Each process
starts with an empty in-memory pipeline cache; Metal's system compiler cache is
not purged. The comparison ran after the soak processes had finished; unrelated
system load was not controlled.

| Frame submission | Baseline `57f7a5c` | Final async path |
| --- | ---: | ---: |
| Median | 0.963 ms | 0.765 ms |
| p95 | 14.381 ms | 2.126 ms |
| p99 | 33.988 ms | 3.484 ms |
| Maximum | 90.300 ms | 6.996 ms |
| Skipped draws / 300 frames | 0 | 32 |

An exploratory run during concurrent resource churn had a **20.742 ms** async
maximum (p99 **3.496 ms**); it is retained in
`target/gpu5-after-spikes-during-soak.log`. The table above is the separate final
comparison, not a claim that competing GPU work can never introduce stalls.

A missing variant skips its draw; no previous incompatible pipeline is used and
no draw is replayed later. The explicit fallback image test checks a black cleared
frame while compilation is pending, followed by the correct white image once the
pipeline is ready. Default compilation remains blocking to preserve existing
`execute` and runner first-frame behavior. Enable `SOJU_GPU_ASYNC_PIPELINES=1`
before constructing the Backend to use the measured nonblocking path.

Archives are tested separately for checksum corruption, truncation, invalid Metal
payloads, disk-budget reduction, queue saturation and compiler failure delivery.
A 64 KiB cache retained 44,416 bytes in the disk-eviction test. A one-byte budget
then removed archives too large to retain. Writes are atomic, failures compile
without the archive, and the exact sources/options/device/OS scope the stable key.
MSL library creation still runs on a persistent archive hit.

## Resource churn

Both runs completed **1,800 seconds**. The baseline checked **16,581,632 pixels**
over 16,193 frames; the final implementation checked **16,449,536 pixels** over
16,064 frames. A distinct texture is uploaded every iteration, 64 colour targets
rotate, and 256 pipeline-state variants cycle. Each frame checks all 1,024 pixels.
The final run also asserts that accounted cache bytes do not exceed its budget.
The final budget is **524,288 bytes**; maximum sampled cache usage was
**490,112 bytes** (478.625 KiB).

RSS and Metal allocations were sampled every minute. Peaks below are sampled
peaks, not continuous high-water measurements. The last sample is at 29 minutes;
both tests then finish at 30 minutes. All 60 samples are in
[gpu5-soak.csv](evidence/gpu5-soak.csv).

| Sample | Baseline RSS MiB | Final RSS MiB | Baseline Metal MiB | Final Metal MiB |
| --- | ---: | ---: | ---: | ---: |
| 0 min | 20.38 | 21.33 | 0.97 | 0.97 |
| 5 min | 33.61 | 21.67 | 18.34 | 0.91 |
| 10 min | 34.30 | 21.39 | 19.53 | 0.91 |
| 15 min | 34.91 | 21.23 | 20.28 | 0.91 |
| 20 min | 38.59 | 21.17 | 22.03 | 0.91 |
| 25 min | 39.91 | 21.20 | 22.66 | 0.91 |
| 29 min | 39.48 | 21.17 | 23.16 | 0.91 |

Peak sampled RSS: **41.86 → 23.36 MiB**.
Peak sampled Metal allocation: **23.16 → 0.97 MiB**.
After warmup, final Metal allocation stays at **950,272 bytes**. The final RSS
samples show no sustained growth. This establishes stability for this workload;
it does not prove the absence of leaks in every workload.

The long run uses implementation `4481d03`; the generated Swift source was
compared byte-for-byte with the checked-in sources. SHA-256:
`c6a5446a08b17c8271526a8d947b21840fdb03a1d60c86f0e65932722992f931`.

The cache budget covers accounted reusable graphics objects. Pipeline/function
storage has a 64 KiB accounting charge per variant because Metal does not expose
its exact allocation size; each pass descriptor is charged 1 KiB. Process RSS, driver storage, compiler jobs, active
submission working sets and pending compute buffers are outside this budget.
Measurements of RSS and `MTLDevice.currentAllocatedSize` are reported independently. Eviction
runs after completion, so a submission can temporarily exceed the cache budget.
Dirty attachments spill before eviction; the full image suite also passes with
zero native and CPU cache budgets. Pressure warning/critical delivery halves the
budget; the test exercises the pressure state without inducing system-wide pressure.

## Ordering and pass encoding

Parallel mode (`SOJU_GPU_PARALLEL_PASSES=1`) encodes disjoint colour-only draw
batches on two to eight independent command queues/buffers and joins all workers
before completing the fence. Texture and pipeline objects can be shared; mutable
upload storage, descriptors, argument tables, heap cursors and recency stamps are
isolated. Clear, query, depth, compute, generation, overlapping-target and spilled
attachment boundaries select serial submission. This is opt-in and is not a
claim of parallel GPU execution or a general dependency-graph scheduler. The
2,000-draw target-switch mix measured 225,131 draws/s with two command buffers,
versus the default-path median of 410,579 draws/s: this small workload does not
amortize worker setup and command-list copying. The default-mode throughput
requirements below do not claim to hold in the opt-in parallel mode.

Shared-depth merging stays ordered: the render pass uses a union of at most eight
colour attachments and remaps fragment outputs and write/blend masks. The
three-draw depth-order fixture uses **one draw encoder instead of three** (four
instead of six encoders including clears). Both paths produce identical expected
images. MRT output permutations, depth occlusion, clear/query boundaries,
generation changes, dimensions and attachment limits are checked. Disable with
`SOJU_GPU_MERGE_DEPTH=0` for the separate-pass comparison.

## Throughput and checks

Final medians of three unchanged 2,000-draw warmed runs, with the soak stopped:

| Mix | Draws/s | Required draws/s |
| --- | ---: | ---: |
| Same state | 700,812 | — |
| Texture switch | 331,707 | 300,000 |
| Constant update | 380,834 | 300,000 |
| Target switch | 410,579 | 100,000 |

The earlier archived measurements and profiling remain in [PERFORMANCE.md](PERFORMANCE.md).

- GPU crate: 31 ordinary tests pass, with Metal API validation and parallel mode
  enabled. The same suite passes with native and
  CPU budgets set to zero; spilled targets correctly fall back to ordered encoding.
- Explicit async fallback, first-seen frame, warmed microbenchmark, mixed
  throughput and resource-churn tests run separately.
- Shader crate: 44 tests pass. Texture crate: seven tests pass.
- All three crates pass offline Clippy with warnings denied.
- Native invariants cover archive integrity, bounded async publication,
  compiler errors, LRU recency, pressure, parallel eligibility and depth-merge
  rejection boundaries.

Local logs live under `tools/switch/gpu/target/gpu5-*`. Reproduce from the GPU crate:

```sh
cargo test --offline
MTL_DEBUG_LAYER=1 SOJU_GPU_PARALLEL_PASSES=1 cargo test --offline
SOJU_GPU_CACHE_BYTES=0 SOJU_GPU_CPU_CACHE_BYTES=0 MTL_DEBUG_LAYER=1 cargo test --offline
SOJU_GPU_ASYNC_PIPELINES=1 cargo test --offline --release --test render pending_pipeline_skips_frame_then_renders -- --ignored --nocapture
SOJU_GPU_ASYNC_PIPELINES=1 SOJU_GPU_DISK_CACHE_BYTES=0 cargo test --offline --release --test render first_seen_shader_frames -- --ignored --nocapture
SOJU_GPU_CACHE_BYTES=524288 SOJU_GPU_CPU_CACHE_BYTES=1048576 cargo test --offline --release --test render resource_churn_soak -- --ignored --nocapture
cargo test --offline --release --test render throughput_state_mixes -- --ignored --nocapture
cargo clippy --offline --all-targets -- -D warnings
```

The baseline was extracted from `57f7a5c` into the ignored, worktree-local
`target/gpu5-baseline`; only the measurement fixtures and a Metal-allocation
reporting field were added. It does not modify another branch/worktree.

## Integration limits

- Async compilation and parallel encoding are opt-in. Async rendering can visibly
  omit first-seen draws and their depth/stencil/query contributions. Shader
  translation/resource capture still happen on the preparation path.
- Compute compilation is synchronous. Compute pipelines are budgeted; pending
  writable compute buffers stay live until readback, then retire with query results.
  Unread attachment spill data is authoritative and
  cannot be deleted to enforce an arbitrary disk cap. Evicting dirty attachments
  can synchronously spill/read back data and cause frame stalls under pressure.
- Parallel mode currently requires the whole batch to qualify and rebuilds its
  private argument tables; small passes can lose throughput from dispatch/setup.
- RSS and Metal allocations include unaccounted runtime/driver storage. A flat
  measured soak is evidence against growth in this workload, not proof that all
  workloads are leak-free. Memory-pressure event delivery itself was not forced.
- Existing format, shader-stage, guest synchronization and runner integration
  limitations in [README.md](README.md) still apply.

## Implementation commits

- `6d2b86d`: background render compilation and checked binary archives.
- `23efe4b`: native/CPU LRU budgets, attachment spill and soak fixture.
- `8c861c3`: independent command-buffer encoding with isolated workers.
- `dd40579`: ordered shared-depth MRT merging.
- `87600c5`: bounded async results, recency tracking and negative cache/scheduler tests.
- `4481d03`: pass-metadata/compute-pipeline budgets and post-readback retirement.
