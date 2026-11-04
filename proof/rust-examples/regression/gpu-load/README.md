# Deterministic capture under load

Ten consecutive runs passed all five original accepted-scene tests (50 passes).
The suite includes the complete Example04 texture interior, with its original
six-unit tolerance. Four continuous SHA-256 load processes ran throughout the
soak, consuming 2030.065 CPU-seconds over 555.869 seconds of test runs.
Other builds and native tests also ran during the soak.

All ten Example04 PNGs have SHA-256
`7eb0efa702f3c94b2b92097636fef18b0c1d8225b5620abcca90bacb3575def4`. The accepted PNG matches this digest.
`results.csv`, `load.csv`, the ten test transcripts and immutable runner/test
binary hashes preserve the evidence. Executables and local input assets are
not stored in git.

The soak copied runner and test executables before starting the load workers;
subsequent builds could not alter them. Its snapshot has the five original
accepted tests. The final expanded six-scene suite was run separately on the
current implementation and passed (`../gpu-accepted.txt`). Capture-mode behavior
is unchanged; the later headless change preserves the original input scheduling
for ordinary non-capture-clock runs.

Reproduce with `python3 scripts/soak-switch-gpu.py`; it now runs the expanded
six-scene suite and also rejects any change in the Example04 capture digest.
