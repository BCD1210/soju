# Parallel runtime continuation

Baseline: `7959847`, branch `feat/switch-core`. Local commits only; no push.
The requested rendering expansion is incomplete. Runtime parallelism, bounded
threaded linker TLS, code-memory aliases and regression measurements are covered
below; this report does not establish commercial-title compatibility.

| Check | Result | Evidence |
| --- | --- | --- |
| Four guest cores on four permanent host threads | PASS: four overlapping hardware runs | cpu-probes.txt |
| Guest virtual timer preemption | PASS: native timer exits in both benchmark modes | cpu-probes.txt, hypervisor.txt |
| 64-worker contention, parallel | PASS: 100/100 | stress-parallel.json |
| 64-worker contention, serialized | PASS: 100/100 | stress-serial.json |
| Four-worker CPU-bound speedup | PASS: 3.038x median, target 3x | cpu.json |
| Cross-core LDAXR/STLXR and LSE LDADD | PASS: exact 40,000 updates for each counter, all five parallel runs | cpu-probes.txt |
| Guest module TLS, migration, zero-fill, kernel TLS-page reuse | PASS: 11 dynamic-fixture checks total | dynamic-tls.txt |
| CreateCodeMemory / ControlCodeMemory and cross-core instruction publication | PASS: 19 checks, 32 code-update rounds | code-memory.txt |
| Native Hypervisor checks | PASS: 22 | hypervisor.txt |
| Core library / kernel / program loader | PASS: 119 / 11 / 4 | core-tests.txt |
| Strict all-target desktop Clippy | PASS | clippy.txt |
| Existing scene pixel checks | PASS: 10 | accepted-scenes.txt |
| Tessellation audit | NOT MET: 14/16 translated, 521/521 decoded | shader-audit.txt |
| MSAA/resolve and Example09 raster | NOT ACCEPTED | rendering.md, rendering-blockers.txt |
| Default corpus | PASS regression: 42 frames / 16 stops / 1 exit, no row changes | corpus.txt |

## Execution and measurements

Each guest core has one permanently thread-affine Hypervisor vCPU. The coordinator
owns kernel state and chooses at most one ready thread per core for each wave.
Worker channels transfer complete saved contexts; completed exits survive resumable
IPC stops. Guest execution releases the memory guard. Before host arena access,
a rendezvous prevents new entries, requests `hv_vcpus_exit` once for active cores,
and waits for them to return. No global mutex remains held during guest execution.
The kernel state is not shared mutably between worker threads.

The native virtual timer replaces per-quantum watchdog threads. Native hardware
executes exclusive and LSE atomics against shared normal memory. Context loads
clear reservations. Page-table and instruction-cache generations are checked
before reentry; every stale core runs the maintenance bridge before using changed
mappings. The code probe writes through an RW alias on core 0 and executes the RX
alias on core 2 after explicit cache maintenance and release/acquire publication.

The CPU probe performs the same four independent 300-million-iteration loops in
both modes, then contends on exclusive/LSE counters. The timed interval starts at
worker release and ends after joins; checksum verification is outside that interval.
The same executable, timer, 5 ms quantum and 8 ms host slices are used. Only
`Runtime.parallel` changes. Five paired runs yield:

- Serialized median: **923.048 ms**.
- Parallel median: **303.807 ms**.
- Median ratio: **3.038x**. Individual paired ratios range from 2.70x to 3.18x.
- Host reports 12 physical CPUs, including eight performance cores; this is a
  four-worker probe on that host, not a claim about an independently tested
  four-physical-core machine.

The SVC-heavy 64-worker test has a different result: serialized median **331.908
ms**, parallel median **499.991 ms** (0.664x, approximately 1.51x slower). Both pass
100/100. Host rendezvous and wave synchronization dominate that workload. Its
assertion count varies with the number of condition-variable wait iterations.
The improvement is CPU throughput, not a universal workload speedup.

The default corpus uses its unchanged one-frame criterion and 15-second timeout.
Every per-title outcome matches 42/16/1 at the baseline. A menu frame does not
establish scene acceptance. The slowest successful first-presentation title in
this run is `graphics__opengl__es2gears` (8.771 seconds; `corpus-timings.json`). A separate ten-presentation
run recorded 8.328 seconds to the first presentation, then **7,467.810 ms mean**
and **7,484.307 ms median** across nine subsequent intervals (7,023–7,940 ms).
Total guest-loop elapsed time was 75.539 seconds. `frame-times.json` retains every
interval; first-presentation timing includes startup and shader compilation.

## Scope still missing

- Scheduling uses coordinated waves and a single kernel owner. SVC/IPC and host
  memory access rendezvous all active cores; independent per-core kernel service
  progression and fine-grained memory access would reduce the measured contention
  regression. General weak-memory litmus coverage and commercial workloads remain
  unverified; the atomic probes cover exclusive increment and LSE LDADD.
- Linker TLS remains a four-module fixture with 256 bytes/module, 128 lifetime
  thread blocks, no TLSDESC, no TLS destructors and no general slot recycling.
  Exception support is exercised on main only; concurrent exceptions are unproved.
- Code memory is heap-backed, exact-size and same-process, with one owner and one
  slave alias in the bounded loader region. Cross-process JIT ownership and handle
  transfer are absent; modern same-owner restrictions are intentionally not modeled
  by this compatibility path. See core/SOURCES.md for the public ABI references.
- All requested new rendering acceptance remains missing: tessellation stages,
  multisampled storage/resolve, and Example09 wide-line/smoothed rasterization.
  The existing ten pixel checks were not weakened.

## Reproduction

```sh
bash tools/switch/testtitles/benchmark_parallel.sh
bash tools/switch/testtitles/stress.sh
SOJU_SERIALIZED=1 bash tools/switch/testtitles/stress.sh
bash tools/switch/testtitles/dynamic/run.sh
bash scripts/test-switch-hv.sh
cargo test --offline --manifest-path tools/switch/core/Cargo.toml --features desktop --lib --test kernel --test program_loader -- --test-threads=1
cargo clippy --offline --manifest-path tools/switch/core/Cargo.toml --features desktop --all-targets -- -D warnings
bash scripts/test-switch-gpu-accepted.sh
env -u SOJU_SWITCH_GPU -u SOJU_SWITCH_AUDIT_SHADERS SOJU_SWITCH_PROOF="$PWD/dist/parallel-corpus" bash scripts/test-switch-examples.sh
```

`benchmark_parallel.sh` builds and signs an isolated probe runner. Use that runner
with `dist/testtitles/code_memory.nro` and an isolated home directory to repeat
the code-memory check. Headless `--frame-times <file.json>` records milliseconds
between presentations, with the first interval measured from guest-loop startup.
Use absolute, resolved home paths. Generated binaries and images remain in ignored
`dist`; the repository contains only original sources and text evidence.

## Local commits

- `4b4899f`: concurrent guest cores, virtual-timer preemption, rendezvous and CPU probe.
- `a2e7bc0`: per-thread guest module TLS, code-memory objects and probes.
- `17a37d5`: fresh rendering acceptance and explicit unmet requirements.
- `2701c4f`: move the unchanged NRO control helper before tests for strict lint checks.
- This report's commit: frame-time instrumentation, reproducible benchmark and evidence.
