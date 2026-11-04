# M2 method progression

Sources: NVIDIA [clb197.h](https://github.com/NVIDIA/open-gpu-doc/blob/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/3d/clb197.h), envytools [gf100_3d.xml](https://github.com/envytools/envytools/blob/f102b82381f3f11cee113d16374c87091db039d9/rnndb/graph/gf100_3d.xml).
Offsets are byte offsets. `handled` means a method completed dispatch, not that a scene passed acceptance. `rejected` means dispatch/action failed. Original FIFO input and expanded MME dispatch are distinguished by `gpu-method` and `gpu-status` lines with `SOJU_SWITCH_TRACE_GPU=1`.

| Method | Public name and fields | Semantics and implementation | Probe outcome |
| --- | --- | --- | --- |
| B197:165c | SET_POINT_CENTER_MODE, V[31:0], OGL=0 / D3D=1; rnndb POINT_RASTER_RULES | Retain mode; reject other values. Point draws are explicitly blocked until centering is verified; triangles and clears are unaffected. | Deko passes this initialization; next 1684=1. |
| B197:0de4 | SET_SM_TIMEOUT_INTERVAL, COUNTER_BIT[5:0]; rnndb WATCHDOG_TIMER | Retain hardware watchdog selection, reject reserved bits. Host execution limits own scheduling; this does not emulate NVIDIA counter timing. | OpenGL passes; next 19cc=1. |
| B197:1684 | SET_PROVOKING_VERTEX, V[0], FIRST=0 / LAST=1 | Flat-interpolated attributes select first/last primitive vertex. | Still rejected. |
| B197:19cc | SET_Z_COMPRESSION, ENABLE[0] | Hardware depth compression enable. | Still rejected. |

No accepted scenes yet. Installed Example01 uses scissored clears; Example02 is the triangle. A software menu capture is not GPU proof. Per-attempt raw logs are under `dist/m2-probe/iterations/`.
