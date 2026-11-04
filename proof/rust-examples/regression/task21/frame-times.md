# Frame times

300 headless presentations per title. Nearest-rank percentiles use the 299 subsequent intervals; the first interval is excluded. Runs are serial with no concurrent builds/tests from this session. System load is not isolated. These are throughput intervals, not display presentation timestamps.

| Title / run | p50 ms | p95 ms | p99 ms | >17.5 ms |
| --- | ---: | ---: | ---: | ---: |
| es2gears / task20 reference | 13.302 | 18.699 | 22.062 | 19/299 |
| es2gears / task21 before | 12.618 | 15.458 | 22.855 | 10/299 |
| es2gears / task21 after | 11.265 | 14.432 | 19.670 | 4/299 |
| dynamic_resolution / task20 reference | 20.455 | 25.828 | 36.060 | 299/299 |
| dynamic_resolution / task21 before | 19.853 | 22.922 | 28.796 | 299/299 |
| dynamic_resolution / task21 after | 16.151 | 19.447 | 21.916 | 43/299 |

**The 17.5 ms requirement is not met:** es2gears p99 and both dynamic_resolution percentiles remain over budget. No warmup intervals were removed beyond the first presentation.

Before: cached release runner copied from the clean `b8bde03` workspace (not rebuilt for this baseline), SHA-256 `04bd195d14cf21fea0927c34ac064a306e7f900601fdcf6e1db330c78ef8e03a`. After: desktop release containing runtime through `e8c3b95`, SHA-256 `caedde31dc169889ad0d2f85351bcf308ac7aa607eb57f11b3d6440cd25d61e7`.

Raw intervals: [frame-times.json](frame-times.json). Numeric summary: [timing-summary.json](timing-summary.json).
