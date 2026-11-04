# Validation at the first accepted GPU scene

Executed on the local Apple Silicon host, 2026-11-04. Logs remain under
`dist/m2-probe/`; real NROs remain outside the repository.

| Check | Result |
| --- | --- |
| Core `cargo test --offline`, `--features hv` | 107 passed; native opt-in tests ignored here |
| Core `cargo test --offline`, `--features desktop` | 109 passed; native opt-in tests ignored here |
| `scripts/test-switch-hv.sh` | 22 signed tests passed |
| `scripts/test-switch-homebrew.sh` with local 2048 | Input/save/restart passed; 358.209 headless fps observed |
| `scripts/test-switch-audio.sh` | Passed; 47,920 nonzero samples consumed |
| GPU crate tests | 28 passed; 3 benchmarks ignored |
| Shader crate tests | 44 passed |
| Python `unittest discover -s tests` | 327 run, OK, 2 skipped |
| Core desktop/all-target Clippy, warnings denied | Passed |
| `scripts/build-app.sh` and strict codesign verification | Passed |
| `scripts/test-switch-examples.sh` | 59: 33 frame, 25 stop, 1 clean exit (unchanged) |
| `scripts/test-switch-gpu-accepted.sh` | Passed twice; 921,600 Example01 pixels checked each time |
| GPU scissored-clear native regression | Passed; every pixel inside/outside checked |

The signed scene test excludes startup/menu frames using `--warmup-ms 1000`,
then captures one frame and finishes normally. The checked-in capture was also
visually inspected against the installed source. Native no-op counters and
ordered method outcomes accompany it. Image acceptance establishes this scene,
not general GPU compatibility or the backend's throughput in this runner.
