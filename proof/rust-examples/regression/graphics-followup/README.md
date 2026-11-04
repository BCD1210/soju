# Graphics follow-up evidence

Run on the local Apple Metal device, starting at `3deb0fb`.

| Evidence | Command |
| --- | --- |
| `core-tests.txt` | `cargo test --offline --manifest-path tools/switch/core/Cargo.toml --features desktop --lib` |
| `gpu-tests.txt` | `cargo test --offline --manifest-path tools/switch/gpu/Cargo.toml` |
| `shader-tests.txt` | `cargo test --offline --manifest-path tools/switch/shader/Cargo.toml` |
| `accepted-tests.txt` | `bash scripts/test-switch-gpu-accepted.sh` |
| `corpus-59.txt` | `SOJU_SWITCH_PROOF="$PWD/dist/gpu-next/corpus-final" bash scripts/test-switch-examples.sh` |
| `shader-after.txt` | `cargo run --offline --manifest-path tools/switch/shader/Cargo.toml --example corpus_audit -- "$HOME/.local/share/soju/switch/examples"` |

`shader-before.txt` runs the same audit example against the shader crate archived
from `3deb0fb` into ignored `dist/gpu-next/baseline`. Static counts are unchanged;
FMNMX.FTZ is exercised by the runtime-generated es2gears shader and the native
shader tests, not by a newly accepted static DKSH program. The audit's synthetic
resource configuration and runtime-GL exclusion are documented in the shader
README. These files are not a claim of complete corpus shader coverage.

The `*-clippy.txt` files use `--all-targets -- -D warnings`, with `--features
desktop` for core. All three crates also pass `cargo fmt -- --check`.

The accepted gears capture is `../../gpu/accepted/opengl-es2gears.png`. The full
seven-scene run reproduces its SHA-256 from the focused test. No input NROs or
shader blobs are stored here.
