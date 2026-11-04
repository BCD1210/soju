# Remaining graphics boundaries

These are failure probes, not accepted scenes. Menu frames and timeout captures
are not pixel evidence. Reproduce with `scripts/probe-switch-gpu.py` and a signed
runner built from `tools/switch/core` with `--features desktop`. Use `--scene` to
select one entry. All probes use the locally supplied, unmodified examples.

| Sample | Current first boundary |
| --- | --- |
| deko Example05 | B197 0x320=0x201; tessellation stages have no Metal lowering |
| deko Example06–07 | CSAA/MSAA/linked-TSC gate; native multisample attachments and resolves are absent |
| deko Example08 | higher viewport clip setup 0xe14=0x05000000 |
| deko Example09 | compute method 0x2b4=0x02000a20; runner QMD integration remains incomplete |
| OpenGL textured_cube, lenny | shader opcode RRO (`0x5c90`), after depth and extent fixes |
| OpenGL dynamic_resolution | unsupported MME opcode, after depth fix |
| OpenGL gpu_console | unsupported MME opcode |
| SDL2 sdl2-simple | GPU mapping kind 0x02 after controller hold request support |

The local Example07 source is mesh loading/lighting with MSAA and sRGB; Example08
is deferred shading/multipass and programs higher viewport state. The corpus
contains six OpenGL NROs and one SDL2 graphics NRO; no separate EGL or instancing
NRO is installed. Existing backend instanced-fetch support has no new corpus
multi-instance pixel acceptance in this change.

OpenGL es2gears is now accepted; see `../accepted/README.md`. The full-scene
probe still exceeds its 30-second budget because it requests 100,000 frames.
Its separate first-presentation test completes and passes source-derived RGB
checks. The four OpenGL depth blockers shared S8Z24 (0x16), now implemented with
correct high-byte stencil packing and native readback tests.

Static DKSH auditing also exposes shader blockers behind the initial method
gates: tessellation programs contain `0xefd0` instructions; lighting/composition
and sinewave compute contain RRO. Neither opcode has been guessed or treated as
a no-op. Example05 additionally requests smoothed four-pixel wireframe lines.

No unclassified state has been accepted as a no-op to pass these boundaries.
Tessellation, MSAA, multiple-viewport selection, compute integration and the
remaining GL/SDL pixel oracles are still outstanding. The full 59-example corpus
frame count is a boot/presentation metric and does not establish these scenes.

SDL HID commands 120/121 follow the public libnx `hidSetNpadJoyHoldType` and
`hidGetNpadJoyHoldType` ABI (resource ID followed by the hold enum on set). Only
vertical/horizontal values are retained; reserved values are rejected. The
currently exposed standard controller has no single-Joy-Con coordinate transform.
Reference: https://github.com/switchbrew/libnx/blob/master/nx/source/services/hid.c
