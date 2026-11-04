# M13 live pacing checkpoint

Private evidence is in `~/.local/share/soju/broodwar-native/3d-proof/m13/` (`runtime/3d-proof/m13/`). No game assets are tracked. Work remains local on `feat/broodwar-3d`.

## Rebuilt baseline

Rebuilt `2683056`, including the previously unmeasured `f0a6dd1` typed frame-catalog cache. `baseline/live-launch.json` records binary SHA256 `c0867d7bdde21766793746e5d0e3327d41c49d24ce6f0313bee5d85d3b244b1f` and LaunchServices nice 0. The private land duel advances with the existing economy/defense controller and a scouting order.

The 100.400-second short run measured cadence **14.77 ms p95**, stereo CPU **5.11 ms p95**, stereo GPU **9.94 ms p95**, and **615 estimated missed refresh slots**. Instrumentation overlaps this window; it is diagnostic, not a ten-minute acceptance run or a controlled comparison with M12's later, larger army.

`baseline/time-profiler.trace`, exported signposts/profile XML, `final-signposts-summary.json`, `final-main-profile.txt`, and `sample.txt` retain the investigation. The initial combined instrument capture stalled and was interrupted; its log and partial trace remain. The subsequent standard Time Profiler template includes Points of Interest and exported successfully.

| Signpost | M12 post-run p95 ms | Rebuilt short-run p95 ms | Rebuilt maximum ms |
| --- | ---: | ---: | ---: |
| Billboard update | 20.32 | 0.773 | 1.641 |
| Batch prepare | 17.21 | 0.004 | 74.613 |
| State apply | 32.35 | 30.847 | 42.389 |

The catalog-cache build no longer shows the large billboard tail in this small-population window. State application still blocks the main thread. Time Profiler attributes 965 ms of sampled main-thread time to state application and 972 ms to overlay rendering. These observations do not prove a large-population speedup.

## Verification in progress

Python: 282 passed and 60 subtests passed. Rust: 14 passed. The first batch-worker build passed 119 XR runtime checks. Final-build verification and the full duel are pending.
