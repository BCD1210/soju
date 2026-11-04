# Default Metal runner

Metal is selected for every title when SOJU_SWITCH_GPU is unset. Set
`SOJU_SWITCH_GPU=off` for the bounded CPU path; `metal` explicitly selects Metal.

The same 59-example corpus, first presentation and 15-second per-example budget
now reports **40 frames / 18 stops / 1 clean exit**, up from default 34/24/1
and explicit Metal 39/19/1. es2gears now presents within the budget.

The slow startup path compiled the Swift worker in every new runner process.
Cargo now compiles it and embeds the resulting native library; launch only
materializes and loads it. Native ABI checks remain unchanged. No generated
native binary is checked in. Build requires Xcode command-line tools, but runner
startup no longer invokes swiftc. This does not cache or substitute guest shaders.

Measured es2gears wall time: 16.26 seconds before, 10.01 seconds after (including
concurrent compilation load). Guest-reported frame time: 16.142 / 9.666 seconds.
The corpus run independently confirms its frame before the 15-second cutoff.

Verification: 113 core library tests pass with default configuration and again
with SOJU_SWITCH_GPU=off. All ten existing source-derived scene checks pass.
The submission test verifies staged bytes, untouched padding, delayed fences,
and native channel fault on failed publication. Native GPU tests pass with Metal
validation. Full logs remain in ignored dist/default-metal-profile.

Reproduce:

```sh
env -u SOJU_SWITCH_GPU -u SOJU_SWITCH_AUDIT_SHADERS SOJU_SWITCH_PROOF="$PWD/dist/default-metal-corpus" bash scripts/test-switch-examples.sh
bash scripts/test-switch-gpu-accepted.sh
cargo test --offline --manifest-path tools/switch/core/Cargo.toml --features desktop --lib
SOJU_SWITCH_GPU=off cargo test --offline --manifest-path tools/switch/core/Cargo.toml --features desktop --lib
MTL_DEBUG_LAYER=1 cargo test --offline --manifest-path tools/switch/gpu/Cargo.toml
```

First presentation is a boot metric, not acceptance of every scene or title.
