# Task21 continuation

Base: `b8bde03`, branch `feat/switch-core`. Changes are committed locally; no push.

**This round is incomplete.** The corpus remains **58 frames / 0 stops / 1 clean exit**.
Example07 is accepted, bringing source-derived scene checks to **13**. Frame-time
costs improved, but the requested ceiling is not met. Tessellation remains
**14/16** translated programs. Live microphone capture could not be established.

## Results

| Requirement | Result |
| --- | --- |
| es2gears <=17.5 ms p95/p99 | Final p95 **14.432 ms**, p99 **19.670 ms**; p99 fails. |
| dynamic_resolution <=17.5 ms p95/p99 | Final p95 **19.447 ms**, p99 **21.916 ms**; both fail. |
| Asynchronous GPU submission and deferred attachment visibility | Not implemented across guest execution. Explicit visibility boundaries still wait for completion. |
| Dirty ranges and fewer copies | Commit comparison visits written byte intervals; tiled writeback updates persistent storage in place, preserving padding. |
| Reusable readback buffers | Single-sample color/depth/stencil planes share one blit submission and a bounded completed-buffer pool. Tightly packed rows use a borrowed view during the synchronous output callback. |
| Guest CPU coherence | An actual AArch64 homebrew probe reads two different native GPU clear results after fences, preserving an adjacent CPU guard. Five checks pass. Removing the GPU clear makes both CPU-read checks fail. |
| Tessellation 16/16 with scene oracle | Not achieved. Full audit: 521/521 decoded instruction occurrences, 14/16 translated programs. Example05 stops at B197 method `0x320=0x201`; no tessellated scene is accepted. |
| Example07 | Inactive generic attributes use the programmed 0000/0001 default from `SET_ATTRIBUTE_DEFAULT`; they do not access a vertex stream. Source-derived teapot projection, per-sample depth, lighting, tone mapping and sRGB resolve pass **777,194 background + 76,058 lighting** checks. |
| Live audin:u capture | Native probe reports authorization undetermined and engine unopened. Host audio enumeration exposes zero input devices. Authorized capture and an actual denied-permission run remain unverified. |

## Implementation and verification

- Dirty interval tracking preserves unchanged gaps, overlapping writes and mapping boundaries.
- Tiled writeback removes the temporary full-layout copy and subsequent copy into the persistent GPU memory image. Short input rejects before mutation; the test checks active bytes, padding and generations.
- The native readback pool reuses buffers only after GPU completion and synchronous consumption. This is not deferred readback across guest CPU execution.
- `SET_ATTRIBUTE_DEFAULT` participates in pipeline identity; the inactive-input regression changes the default across successive draws and leaves its selected stream disabled/unmapped.
- The guest coherence probe exposed missing address reservation teardown. `NVGPU_AS_IOCTL_FREE_SPACE` now frees exact, fully unmapped reservations; occupied, partial and repeated release attempts reject. Remapping and address reuse are checked.
- Core desktop library: **142 passed**.
- GPU: **36 active render tests passed**, five existing ignored; **6 library, 2 compute, 4 copy, 13 state**, and both native harnesses passed.
- Render tests also pass with native caching disabled and Metal API validation enabled.
- Texture layout: **8 passed**. Strict all-target core/GPU clippy passed.
- Accepted scenes: **13 passed**, including Example07, with Metal API validation. The same Example07 oracle fails against the baseline runner at the inactive-attribute boundary.
- Default corpus: all **59 processes exit 0**, with 58 first frames and one clean non-frame exit.

## Remaining work

1. Remove eager mapped-memory snapshot comparisons using CPU write tracking that also covers hardware-executed guest writes and physical aliases. Host-only generations are insufficient.
2. Add attachment ownership/fence state and protected guest mappings or another complete CPU-access mechanism before deferring writeback. CPU loads, host service reads, map/unmap and syncpoint publication must all wait for and expose the right data. Current coherence is proven only for the retained synchronous boundary.
3. Keep GPU attachment layouts resident across frames and avoid guest layout conversion until a real visibility requirement. Native submission still waits; multisample transfers retain their separate completion path.
4. Implement tessellation setup, control/evaluation patch ABI, ISBERD/per-vertex addressing, patch constants/factors, compute factor generation and Metal post-tessellation functions. A tessellated scene oracle, including the sample's wireframe behavior, is still absent. No translation-count workaround was added.
5. Connect an input device and complete permission handling in a live session; run the native timing probe and the audin:u guest path for both authorized and denied states. Current silent-buffer timing uses a simulated clock, not AVAudioEngine callbacks.

## Evidence and reproduction

- [Per-title before/after](per-title.md), [frame times](frame-times.md), [profiling](profiling.md).
- [Shader audit](shader-audit.txt), [guest CPU coherence](coherence.txt), [microphone status](microphone.txt), [commits](commits.md).
- `bash scripts/test-switch-gpu-accepted.sh` runs all 13 accepted scene checks with locally supplied public examples.
- `bash scripts/test-switch-examples.sh` runs the default installed corpus.
- `bash tools/switch/testtitles/gpu/build.sh` builds the original coherence fixture into ignored `dist/testtitles/`. Run it with the signed core `test_title` example to collect the five checks.
- Build the live microphone probe with:

```sh
clang -fobjc-arc -fblocks -Wall -Wextra -Werror \
  -framework Foundation -framework AVFoundation \
  tools/switch/core/native/microphone.m tools/switch/core/tests/microphone_probe.m \
  -Wl,-sectcreate,__TEXT,__info_plist,tools/switch/core/native/microphone.plist \
  -o dist/task21/microphone-probe
dist/task21/microphone-probe
```

The microphone probe logs authorization, buffer counts and timing only. No audio
samples are persisted. Captures, full test logs, binaries and Instruments recordings
remain under ignored `dist/task21/`; this report contains aggregate evidence and
public-homebrew audit results only. Public contracts are recorded in the core/GPU
source ledgers.
