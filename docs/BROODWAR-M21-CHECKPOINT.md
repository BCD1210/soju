# Brood War M21 checkpoint

**M21 is not fully qualified.** The final duel passes latency and the requested
cadence p95 target, but desktop refresh/latency, stress latency, and the stricter
zero-missed-slot qualification remain open. M21 starts from `2c9ead7` on
`feat/broodwar-3d`. Private evidence is under
`~/.local/share/soju/broodwar-native/3d-proof/m21/`. Results below distinguish
GPU completion, estimated XR display time, and actual desktop presentation.
None is physical photon telemetry. No shared OXRSys source or service was changed.

## Controlled duel comparison

Separate source archives and signed private binaries were built for `1d08cde`
and `2c9ead7`, without checking out or rewriting repository history. Both used
`Soju-Duel-Land.scx`, the same controller in `compare_duel.py`, a 300-second
requested window, default settings, and the same private runtime and simulator.
The original pair ran back to back; no builds or tests overlapped measurement.
`comparison-summary.json` records hashes and native frame ranges.

| Build / run | Measured seconds | Display estimate p95 | GPU completion p95 | Cadence p95 | First native frame |
|---|---:|---:|---:|---:|---:|
| `1d08cde` / old | 302.48 | 57.11 ms | 56.35 ms | 11.98 ms | 1391 |
| `2c9ead7` / head | 302.62 | 59.95 ms | 53.52 ms | 11.95 ms | 1389 |
| `6a44614` / midpoint | 296.27 | 57.51 ms | 56.57 ms | 11.97 ms | 1299 |
| `1d08cde` / repeat | 300.74 | 56.54 ms | 55.08 ms | 11.96 ms | 1421 |
| `d780b6f` / late | 300.51 | 58.72 ms | 58.72 ms | 11.97 ms | 2033 |

The matched first pair did not reproduce the original 68.30 → 84.77 ms gap.
HEAD was 2.84 ms worse in estimated display latency while GPU completion improved
2.83 ms. This smaller difference remains unresolved; it is not a clean negative
regression finding. The midpoint and late-build probes do **not** establish an
offending commit: the midpoint recorded 79 attacks while the first pair recorded
none, and the late build started substantially later. No M20 change was reverted
on this inconclusive evidence. The midpoint's frozen metrics also cover less
than the requested 300 seconds; it is not a fully matched qualification window.

The original full M19 and M20 duels were not equivalent gameplay windows. Their
first sampled native frames were 1585 and 5010; attacks were 217 and 567, deaths
24 and 54, and mean visible units approximately 31.6 and 36.5. These differences
are a confounder, not proof that gameplay caused the regression. The early
comparison cannot exclude a regression that appears during sustained combat.

Each comparison retains preflight/midrun `top` snapshots. The first pair used
single samples, whose per-process CPU percentages are not useful; the additional
HEAD sample and subsequent runs use two samples one second apart. Unrelated
applications, WindowServer, the Android emulator and OrbStack remained running.
Memory pressure included roughly 15 GiB compressed. No exact host-load cause
was established, and neither early build reproduced the slow full-duel result.
`host-clock-probe.json` measured only 0.144 ms of monotonic/absolute clock drift
in about 61 seconds; it does not explain the display-estimate differences.

## Desktop investigation

M20's initial display-link experiment rendered and presented about 100 Hz, but
latency increased from 78.39 to 97.56 ms. Deferring acquisition of the latest
scene packet toward the render deadline reduced that to 88.30 ms, still failing.
Its two-drawable experiment failed because a pending MTKView draw callback tried
to acquire a drawable after CAMetalDisplayLink took ownership. These are distinct
queueing and ownership problems; native refresh alone was not a latency fix.

M21 retains failed experiments in `stress-desktop-*/`, including synchronized
CADisplayLink acquisition, transaction presentation, a direct CAMetalLayer,
render-queue submission, deadline deferral, and two versus three drawables.
Transaction presentation reached 147.68 ms; a CADisplayLink target-time experiment
reached 325.57 ms. Neither is retained. A 100 Hz render loop with deferred
CAMetalDisplayLink submission completed only about 65–68 presentations per second.
Changing NSView to MTKView or increasing the drawable pool did not remove this
ceiling. Render rate must not be reported as completed presentation rate.

The final supported implementation is `8367024`: MTKView manages sizing, its
ordinary drawing entry point is disabled, and only CAMetalDisplayLink supplies
drawables. The link uses the containing window's screen maximum refresh and a
matching preferred frame-rate range; MTKView's preference is kept in agreement.
Vsync stays enabled, the layer has two drawables, transaction presentation stays
disabled, and admission permits only one deferred draw at a time. Submission
runs on the main queue with a 4 ms margin before the rendering deadline.

Additional synchronous and MTKView-preference experiments did not remove the
presentation ceiling. Explicit `present(atTime:)` with a CAMetalDisplayLink
provided drawable raised `CAMetalDrawableInvalidOperation` before measurement;
that unsupported experiment is removed and its failure log retained. The exact
cause of the current completed-presentation ceiling has not been established.
The `desktopDrawableWait` stage now measures command-buffer acquisition only:
CAMetalDisplayLink provides the drawable, so it is not a `nextDrawable` wait.
The generic metrics report's nominal 90 Hz field is inherited XR instrumentation;
the desktop's requested 100 Hz and measured presentation count are reported
separately. The final verifier requires at least 95 completed presentations per
second on this screen, as well as the latency gate.

Apple defines CAMetalDisplayLink's `targetTimestamp` as a rendering deadline and
`targetPresentationTimestamp` as an estimated display time. They are not
interchangeable. See [update timing](https://developer.apple.com/documentation/quartzcore/cametaldisplaylink/update)
and [transaction presentation](https://developer.apple.com/documentation/quartzcore/cametallayer/presentswithtransaction).

## XR scheduling

`c15d9b0` requests a preemptible Mach time-constraint policy on the dedicated XR
frame thread, using the runtime's negotiated period, a computation allowance of
at most 2 ms, and a constraint of at most 3 ms. It does not change runtime timing,
render quality, capture defaults, texture limits, or native simulation speed.
The policy return code is logged. `87e6085` excludes the main thread, preserving
the SceneKit fallback's main-queue scheduling. Preliminary stress runs improved
cadence p95 to approximately 11.18–11.21 ms with no missed refresh slots.
The final ten-minute run below determines whether this holds during combat.

## Final qualification

All final runs use source `8367024`, default settings, nice 0, capture disabled,
allocation tracing disabled, no profiler, and no overlapping builds or tests
launched by this task. External compiler activity was observed around the
ground stress run, as recorded below.
`build-final-evidence.json` records zero staged-source mismatches and these hashes:

- Desktop: `4232f4bd949643fa138b94b4e89120f032a8cd7293fc898baaf10359b93a4a30`.
- XR: `e552ad26c07c8f5ed65ffc4986921ccfb56b79f59df9071c5d1d21e1ad21a770`.

The final duel uses a private copy of the exact XR binary. It completed **610.79
measured seconds**, **607.63 native-sample seconds**, and 14,428 native frames,
with 290 attacks, 262 damage events, and 30 deaths. All samples retained fresh
tracking, fresh scene publication, and a surviving owned army. Peak texture
residency was 1,396.94 MiB under the default 1,536 MiB limit, with zero rejections.

- Native-to-display-estimate p95: **69.85 ms — PASS** against 70 ms.
- Cadence p95: **11.19 ms — PASS** against the requested 12.0 ms target.
- Missed refresh slots: **4**. The pre-existing stricter cadence qualification
  requires zero, so `qualification.json` still records `cadencePass: false`.
- Combat/survival qualification: **PASS**.

This is evidence for the final build's complete run, not proof of the cause of
M20's regression. The smaller matched-window difference remains unresolved.

Python passed **286 tests plus 60 subtests**, Rust passed **18 tests**, and
final desktop/XR/fallback runtime checks passed **266/271/271**. Logs are
`tests-python-final.log`, `tests-rust-final.log`, and
`runtime-{desktop,xr,fallback}-final.{log,json}`.

Every sampled owned army contained 125 units. Native move acknowledgements and
movement, default settings, binary hashes, fresh XR tracking/publication, and zero
texture rejections are asserted by `verify_final.py`. Possession setup uses
ordinary grip/select controller input and verifies an owned Marine; it does not
mutate viewer state directly.

| Final run | Seconds | Display latency p95 | Frame interval p95 | Missed XR slots |
|---|---:|---:|---:|---:|
| `stress-tabletop-final/` | 45.53 | **70.10 ms — FAIL** | 11.18 ms | 0 |
| `stress-ground-final/` | 41.52 | **187.71 ms — FAIL** | 11.17 ms | 0 |
| `stress-possess-final/` | 44.67 | **416.54 ms — FAIL** | 11.18 ms | 1 |
| `stress-desktop-final/` | 45.11 | **88.61 ms — FAIL** | 10.96 ms | n/a |

Desktop rendered at 99.22 Hz but completed only **66.44 presentations/second**,
so the native-refresh presentation requirement is not met. It uses actual
compositor timestamps. XR rows use estimated display time. All final source/test
checks passed; these performance failures remain open. The combined
`final-qualification.json` deliberately records `allPerformanceGatesPass: false`.
A read-only CUA inspection of the final desktop process during warm-up showed
the tabletop terrain and clustered army rendering; no view input or resize was
applied to the timed run (`desktop-final-visual-qa.json`).

The ground and possession latency spikes occurred alongside a new host-load
surge. Ground's native-to-bridge p95 was 89.09 ms and native-to-packet 152.78 ms;
possession reached 119.34 and 318.38 ms respectively. The delay is upstream of
render cadence. The immediate post-ground snapshot showed load average 128.68;
a separate process observation found Swift compilation and linking. Possession's
warm-up snapshot had 47 runnable processes and only 2.88% CPU idle. Subsequent
snapshots recorded unrelated Java/simulator activity and load average 261.23.
No such jobs were launched or stopped by this task. These observations support
host contention as a confounder, but do not prove a complete causal attribution
or excuse the failed measurements. See `process-after-ground-final.txt`,
`ground-postrun-observation.json`, `process-possess-warmup.txt`, and
`process-before-ground-repeat*.txt`. The source of the short-lived compiler jobs
was not captured before they exited.


A diagnostic ground repeat was planned but not started: two preflight snapshots
still showed unstable heavy load (1-minute averages 261.23 and 82.64; the latter
had CPU idle readings of 5.7% and 35.77%). `ground-repeat-status.json` records the
blocked repeat. No failed final measurement is replaced by an earlier passing
result. Obtaining a stable host window, isolating the remaining small matched
comparison difference, and resolving the actual desktop presentation ceiling
remain necessary before claiming completion.

## Repository boundaries

Implementation steps and this checkpoint are committed locally; nothing is
pushed. Game assets, fixture/controller copies, traces and build products remain
private. `tools/broodwar_native/target/` is never staged. The existing edit to
`docs/BROODWAR-MODEL-QUEUE.md` is untouched. Only the private Brood War bottle,
viewers and private simulator are operated; unrelated processes remain running.
