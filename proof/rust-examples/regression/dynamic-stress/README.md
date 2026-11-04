# Dynamic modules, threading, memory and viewport regression

Baseline: `2c1b9a4`, branch `feat/switch-core`. All changes are local commits.
Original probe sources are in `tools/switch/testtitles`; generated binaries and
full scene traces stay under ignored `dist`. No supplied proprietary content is
used by these probes.

| Check | Result | Evidence |
| --- | --- | --- |
| Four NSOs, guest MOD0/.dynamic linker | 8 PASS | dynamic-final.txt |
| Cross-module calls/data, six relocation forms, TLS templates, init/fini | PASS | dynamic-final.txt, fixture README |
| Cross-module C++ exceptions, RTTI, destructors | PASS | dynamic-final.txt |
| 64 workers plus main; four logical cores; contention | 100/100 PASS, 567 checks/run | thread-final-100.txt |
| Repeated migration back to resident cores | 5 PASS | context-roundtrip.txt |
| Physical/heap/transfer/process-code memory; 3 GiB heap | 44 PASS | memory.txt |
| Native context/cache/Hypervisor checks | 22 PASS | hv-final.txt |
| Core library / kernel / program loader | 119 / 9 / 4 PASS | core-final.txt, memory-tests.txt |
| Render tests, including all 16 viewport/scissor slots | 28 PASS, 5 existing ignored | render.txt |
| Accepted guest rendering suite | 10 PASS | accepted-final.txt |
| Existing boot probes and synthetic package persistence | PASS | probes-final.txt |
| Shader audit | 14/16 translated, 521/521 instructions decoded | shader-audit.txt |
| Default corpus | 42 frames / 16 stops / 1 exit | corpus-final.txt |

The default corpus has **no per-title outcome changes** from the baseline's
42/16/1. Its default 15-second timeout and one-frame criterion are unchanged.
A default corpus frame can be a menu; it is not scene acceptance.

## Remaining gaps

- The guest linker is a real, bounded four-module fixture, with single-thread TLS
  blocks and exception-runtime adapters. It does not establish arbitrary runtime
  linking, per-thread dynamic TLS, TLSDESC, ASLR, symbol-version/interposition rules
  or arbitrary dependency ordering. See the fixture README for exact limits.
- Four permanent vCPUs execute saved guest contexts, but the process guard still
  serializes execution. Simultaneous four-core execution and weak-memory stress
  remain unproved. Priority inheritance, complete core-mask sentinel semantics,
  resource accounting and general scheduling fairness are not established.
- Memory probes cover transfer creation/locking/close and MapProcessCodeMemory,
  not CreateCodeMemory/ControlCodeMemory or cross-process transfer mappings.
  Physical unmap requires the exact allocation. Code aliases are same-process,
  heap-backed and constrained to the loader region.
- Tessellation remains **14/16**, not 16/16. Example05 stops at patch configuration
  B197 0x320=0x201; control/evaluation stage invocation, per-patch attributes and
  ISBERD semantics still need implementation and independent output checks.
- Example06/07 still stop on MSAA state. Multisampled storage, attachment lifetime,
  sample masks and guest resolve semantics are not implemented.
- Example08 is deferred shading, not a viewport-array proof. It still rejects an
  inactive/swapped vertex attribute. Viewport arrays are instead verified with
  original method streams, original shaders and exact pixels.
- Example09 still stops at polygon smoothing 0x1658=1; wide-line raster support is
  also missing. Its previously supported compute path does not establish raster.

`rendering-blockers.txt` is a fresh run of Example05–09. The scene probe now waits
one second before menu navigation; the old 100 ms start could lose the first
button transition while traced startup was still completing and select the wrong
scene. These are explicit stop records, not acceptance claims.

## Local commits

- `923f478`: guest linker, original modules, cross-module exception/RTTI probes.
- `22449d9`: four saved vCPU contexts, 64-worker synchronization stress, timeout fix.
- `0e13904`: physical/process-code memory and 3 GiB heap probe.
- `a8a8047`: revision-aware resident-context resume optimization.
- `5240767`: sixteen shader-selected viewport/scissor arrays and pixel oracle.
- `59fb95f`: repeated migration round-trip probe.

This report and corrected scene-probe startup timing form the final evidence commit.
