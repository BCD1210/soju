# Reproduction and limits

Command: `python3 scripts/probe-switch-gpu.py dist/m2-probe/iterations/runner --output dist/m2-probe/final-methods`.
The isolated binary was built with `cargo build --offline --release --manifest-path tools/switch/core/Cargo.toml --features hv`, copied, and signed with `tools/switch/core/hypervisor.entitlements`.
Every run uses a fresh save home, headless mode, Metal, method tracing and a
30-second timeout. Temporary first-frame captures are discarded, because a deko
software menu is not scene acceptance. These logs contain dispatch results, not
proof of rendered images. `summary.txt` reports the final failure per run.

The `.methods` files preserve ordered dispatch outcomes, including expanded MME
writes; macro parameter words appear in the raw FIFO logs under
`dist/m2-probe/final-methods/*.log`. Raw FIFO entries after the rejected method
were submitted but were not executed. Reference hashes identify the exact public
headers used to classify the methods. No proprietary inputs were used.
