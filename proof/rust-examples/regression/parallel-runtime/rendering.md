# Rendering acceptance — incomplete

No new rendering feature is accepted in this continuation. The existing ten
scene tests still pass their pixel oracles without tolerance changes. Fresh
Example05/06/07/09 probes exercise scene navigation, not just menu presentation.

| Requested result | Actual result | Evidence |
| --- | --- | --- |
| Tessellation 16/16 | NOT MET: 14/16 translated; 521/521 instructions decoded | shader-audit.txt |
| Example05 tessellation pixels | NOT ACCEPTED: B197 0x320=0x201 | rendering-blockers.txt |
| MSAA and resolve pixels | NOT ACCEPTED: Example06/07 reject multisample setup | rendering-blockers.txt |
| Example09 raster pixels | NOT ACCEPTED: polygon smoothing 0x1658=1 | rendering-blockers.txt |
| Existing accepted scenes | PASS: 10 source-derived pixel tests | accepted-scenes.txt |

The native path still lacks a control-stage pass, tessellation-factor buffers,
patch/evaluation-stage interfaces, multisampled attachment storage and guest
resolve semantics. Example09 additionally needs wide-line rasterization and
smoothing; its working compute dispatch does not establish those raster results.
Unsupported state remains rejected. The scene capture harness was not relaxed.
