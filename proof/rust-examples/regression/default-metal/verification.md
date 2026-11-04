# Final verification and remaining requirements

Verified implementation commits: 91739ca (default and startup), 0218ea0
(bounded compute bridge), 6c8152b (desktop activation, packaging, cache path).

- Final uninstrumented default corpus: **40 frames / 18 stops / 1 clean exit**.
  Same 59 NROs, first presentation, unchanged 15-second per-example timeout.
  Every frame in the earlier 39/19/1 Metal run remains a frame; es2gears is added.
- Final corpus es2gears first frame: **7.175482 seconds**. The earlier wall-time
  comparison was 16.26 seconds before build-time worker compilation and 10.01
  seconds after, under concurrent compilation load.
- Core library: **114 passed**. CPU opt-out was separately verified at the default
  change (113 tests before the new compute integration test).
- Existing scene suite: **10 passed**, including the fixed-pose source-derived
  gears oracle. No new deko scene acceptance is claimed.
- GPU suite: **40 passed**, 5 intentionally ignored throughput/soak tests, with
  MTL_DEBUG_LAYER=1. Core library and GPU all-target Clippy pass with warnings denied.
- Static symbolic-texture shader audit: **14/16 translated**, **521/521 decoded**.
- Soju Add game → Play: foreground Metal game window captured in
  ../../../app-launch/app-and-game.png (see proof/app-launch/README.md).
  Signed app seal verifies after launch; installed runner is ad-hoc signed.

## Requirements still not fulfilled

| Requested work | Current boundary |
| --- | --- |
| Both tessellation stages / 16 of 16 / Example05 | Still unsupported; Example05 stops at B197 0x320. Patch control-point ABI, control compute pass, factors and post-tessellation vertex execution remain absent. |
| MSAA and resolve / Examples06–07 | Still rejects multisample configuration; sample storage, persistence and resolve writeback are absent. |
| Viewport arrays and multipass / Example08 | Stops at inactive/swapped vertex attributes before later passes; viewport routing/composition acceptance remains absent. |
| Compute / Example09 | Dispatch now executes through the backend; subsequent raster setup stops at polygon smoothing 0x1658=1. The source also requests a 16-pixel line strip. Sinewave pixel acceptance remains absent. |

These are substantive remaining backend implementations, not accepted scenes.
The complete requested rendering expansion is unfinished. Final scene probes
are in remaining-scenes.txt. Only screenshots, summaries, source references and
hashes are retained as proof; local inputs and native build products stay outside
tracked files. No push was performed.
