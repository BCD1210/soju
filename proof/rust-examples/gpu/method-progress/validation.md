# Regression results

All commands completed successfully. Logs remain under `dist/m2-probe/`. Signed tests run separately from ordinary Cargo tests.

| Log | Result |
| --- | --- |
| `core-hv-next.log` | 102 passed; 23 ignored (signed suites executed separately where applicable) |
| `core-desktop-next.log` | 104 passed; 24 ignored (signed suites executed separately where applicable) |
| `gpu-next.log` | 25 passed; 2 ignored (signed suites executed separately where applicable) |
| `shader-next.log` | 44 passed; 0 ignored (signed suites executed separately where applicable) |
| `signed-hv-next.log` | 22 passed; 0 ignored (signed suites executed separately where applicable) |
| `homebrew-next.log` | 1 passed; 0 ignored (signed suites executed separately where applicable) |
| `audio-next.log` | 1 passed; 0 ignored (signed suites executed separately where applicable) |
| `python-next.log` | Ran 327 tests in 50.845s; OK (skipped=21) |
| `python-switch-next.log` | Ran 99 tests in 4.096s; OK (skipped=3) |
| `clippy-next.log` | desktop / all targets / warnings denied: passed |
| `app-next.log` | Built app; deep strict signature verification passed; app not launched |

Corpus rerun: **33 frame / 1 exit0 / 25 stop**, unchanged.

No accepted GPU scenes. No scene pixel tests are claimed.

Implementation commits: `701d5f7`, `c2637c3`, `d0aabe1`.
Remote `origin/feat/switch-shader` fetched at `ed0f136`; its content is already in the current branch. Newer local shader-branch commits were not pushed at this check. No commits were pushed by this work.
