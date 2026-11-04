# Frame times

300 headless presentations per title; mean and nearest-rank percentiles use the 299 subsequent intervals. First-present latency is reported separately. Before: runtime `3a9bb1a`; after: `98da0ac` (same runtime as `7543d99`). Both sets ran serially with no concurrent builds/tests from this session; system load is not isolated. These are short throughput probes, not display-present timestamps or a soak. The new desktop swapchain is not exercised by headless timing.

| Title / run | Mean ms | p95 ms | p99 ms | >17.5 ms | First present ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| es2gears / before | 15.307 | 25.450 | 32.544 | 43/299 | 1235.178 |
| dynamic_resolution / before | 24.144 | 31.976 | 38.676 | 299/299 | 1395.270 |
| es2gears / after | 14.106 | 18.699 | 22.062 | 19/299 | 788.666 |
| dynamic_resolution / after | 21.396 | 25.828 | 36.060 | 299/299 | 681.233 |

The requested p99 ceiling is **not met**. Synchronous guest attachment readback and snapshot/commit costs remain. Separate desktop runs presented 120 frames and exited successfully under Metal validation: es2gears 1.836 s; dynamic_resolution 2.557 s (first-to-exit benchmark durations, not percentile measurements).

Raw samples: [frame-times.json](frame-times.json).
