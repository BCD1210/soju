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
