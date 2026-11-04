# Profiling

Both 300-frame dynamic_resolution Time Profiler recordings completed with process exit 0. Signposts use `org.soju.switch` / `PointsOfInterest`. These separate instrumented runs are for attribution; they are not the percentile runs. Inclusive intervals overlap and must not be added into wall time.

| Stage | Before calls | Before ms | After calls | After ms |
| --- | ---: | ---: | ---: | ---: |
| snapshot | 302 | 903.023 | 302 | 918.755 |
| prepare | 19500 | 873.278 | 19500 | 857.937 |
| submit | 302 | 898.264 | 302 | 811.710 |
| readback | 302 | 2087.855 | 302 | 1318.379 |
| commit | 302 | 545.237 | 302 | 298.911 |
| present | 300 | 604.507 | 300 | 620.239 |
| gpu_completion | 302 | 442.592 | 302 | 282.841 |
| native_readback | 302 | 884.724 | 302 | 294.522 |
| attachment_completion | 0 | 0.000 | 302 | 130.341 |

Baseline CPU leaf samples prominently include memory comparison (935 ms), memory move (921 ms), and tiled writeback (668 ms). These are sampled CPU time, not exclusive frame latency. The exact leaf aggregates and signpost counts are in [profile-summary.json](profile-summary.json).

Local recordings and exports remain under ignored `dist/task21/`: `before-dynamic-time.trace`, `after-time.trace`, `before-metal.trace`, `after-metal.trace`, the stack XML exports and signpost XML exports. `before-time.trace` was an invalid initial launch with a mistaken title path and is excluded.

Reproduce with `SOJU_PROFILE=1 xcrun xctrace record --template "Time Profiler" --no-prompt --output <new.trace> --launch -- <signed-runner> --headless <public-example.nro> --frames 300 --capture <png>`. Use `--template "Metal System Trace"` for the GPU recording. Readback now has an `attachment_completion` interval.

## Metal System Trace

Both recordings completed and the target exited 0 (before 10.761 s, after 9.194 s recording durations). Application intervals are filtered by the launched runner PID; unrelated system/application activity is excluded. The trace contains competing GPU activity, so this is not an isolated device benchmark.

| Runner event | Before count | Before ms | After count | After ms |
| --- | ---: | ---: | ---: | ---: |
| Encoding | 1491 | 893.416 | 1505 | 804.925 |

Exact filtered totals: [metal-summary.json](metal-summary.json). Completion waits are measured by the signposts above; the encoding trace does not establish asynchronous execution across guest CPU runs.
