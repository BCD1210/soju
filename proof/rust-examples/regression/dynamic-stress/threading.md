# Thread context and contention verification

The guest probe creates 64 workers plus the main thread, using all four core IDs,
then migrates each worker. Each worker verifies a unique SIMD value and libnx TLS
value across a sleep. A mutex-protected counter with deliberate yields checks
1,024 updates; condition variables, address arbitration, events, exit/join and
three timeout paths are exercised. `thread-100.txt` records 100 successful runs,
567 checks each. The completion transcript is `thread-stress.txt`.

The host now keeps four permanent, thread-affine vCPUs and saves full guest
contexts. It does not run them simultaneously: the process memory guard still
serializes guest execution. This proves scheduling/affinity and contention under
that model, not parallel execution or weak-memory races. The limit is 128 live
guest threads; resource limits and arbitrary scheduler fairness remain bounded.

`context-negative.txt` records the expected failure when Q31 was deliberately
omitted from save/restore. The original context code was restored and the probe
passed again. The baseline stopped at unsupported CreateEvent; after adding that
and increasing the thread limit, the old per-thread vCPU design exhausted the
host vCPU limit. The four-worker model removes that resource dependency.

Condition-variable timeout previously reacquired the mutex inside the kernel,
causing libnx to deadlock while reacquiring it a second time. Timeout now leaves
ownership with the caller; successful signaling still reacquires before returning.
The old kernel test encoded the incorrect timeout behavior and was corrected
against the public libnx contract:
https://github.com/switchbrew/libnx/blob/master/nx/source/kernel/condvar.c

`kernel.txt`: 9 kernel integration tests pass. `hv.txt`: 22 native Hypervisor
smoke tests pass. `asan-thread.txt`: the real guest probe passes in the instrumented
Rust host with leak detection. Miri could not execute `sys_icache_invalidate`;
no Miri pass is claimed. The C shim keeps the SDK's SIMD vector ABI in C and
exposes only initialized, fixed-size arrays to Rust.

Reproduce after building probes:
`bash tools/switch/testtitles/stress.sh`
