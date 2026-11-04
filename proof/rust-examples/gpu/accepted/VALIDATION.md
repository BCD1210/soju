# Validation: five accepted GPU scenes

Executed on the local Apple Silicon host, 2026-11-04. Detailed logs are under
`dist/task10-validation/`; NROs and original texture assets remain outside the repository.

| Check | Result |
| --- | --- |
| Core offline tests, `--features hv` | 113 passed; opt-in native tests ignored here |
| Core offline tests, `--features desktop` | 115 passed; opt-in native tests ignored here |
| Signed Hypervisor tests | 22 passed |
| Signed local 2048 input/save/restart | Passed; 484.864 headless fps observed (not a GPU benchmark) |
| Signed AudioUnit tone | Passed; 47,920 nonzero samples consumed |
| GPU crate, Metal debug layer enabled | 35 passed; 5 benchmarks ignored |
| Shader crate | 44 passed |
| Texture crate | 7 passed |
| Python full suite, installed venv and local 2048 | 327 run, OK, 2 skipped |
| Core desktop/all-target Clippy, warnings denied | Passed |
| App build and strict signature verification | Passed; isolated `dist/task10-app/Soju.app` |
| `scripts/test-switch-examples.sh` | 59: 33 frame, 25 stop, 1 clean exit (unchanged) |
| Serial native GPU acceptance | Five scenes passed twice; 4,516,884 source-derived pixel checks per run |

The native acceptance set is deko Examples01–04 and OpenGL simple_triangle.
Each checked-in capture was visually inspected; ordered dispatch/no-op outcomes
accompany it. Startup/menu frames are excluded using `--warmup-ms 1000`.
Cubes are initial-pose checks; depth storage/comparison, quad coverage, split
texture destinations, perspective interpolation and bounded DMA have separate
native/unit regressions.

An initial five-scene concurrent run failed Example04's first-pose tolerance
because compilation stalls advanced its animation. The script now runs native
scenes serially; two complete serial runs passed without increasing tolerances.
This is not an animation-timing or multi-frame acceptance claim. Synchronous
pipeline compilation is selected for translated scenes. The runner still creates
a backend per submission, so shader-agent cache/throughput measurements do not
establish runner performance.

Shader branch merges include `87600c5` and later `4481d03`; there were no further
commits on `feat/switch-shader` at the final check. All work is local, not pushed.
