# Stop reasons at the first failing boundary

Default corpus: 42/16/1 before, 44/14/1 after (frames/stops/clean exits).
These are first stops, not claims about code unreachable beyond them.

| Title | Before | After |
| --- | --- | --- |
| alarm-notifications | Missing service `notif:a` | Unchanged |
| app_controldata | `ns:am2` command 7989 (read-only control-data interface) | Unchanged |
| applet__app-playstats | Missing service `pdm:qry` | Unchanged |
| audio__echo | Missing service `audin:u` | Unchanged |
| graphics__deko3d__deko_console | AS-GPU unmap ioctl `0xc0084105` | B197 `SET_FILL_VIA_TRIANGLE` (`0x113c=2`, FILL_BBOX) |
| graphics__opengl__gpu_console | TEXS array resource type/handle mismatch at shader offset `0x48` | Unchanged |
| graphics__opengl__textured_cube | TSC control `0x00026000`: rejected inactive comparison function | Frame presented |
| hid__abstracted-pad | Missing service `hid:dbg` | Unchanged |
| hid__hdls | Missing service `hid:dbg` | Unchanged |
| hid__irsensor | Missing service `irs` | Unchanged |
| hid__notification-led | Missing service `hid:sys` | Unchanged |
| hid__sevensixaxis | HID command 306 | Unchanged |
| network__ldn | Missing service `ldn:u` | Unchanged |
| network__lp2p | Missing service `lp2p:app` | Unchanged |
| power | PSM command 0; further required queries 1, 13, 14, 15 | Frame presented |
| usb__usbhs | Missing service `usb:hs` | Unchanged |

The shared `hid:dbg` service blocks two titles; each other current first boundary blocks one.
Its virtual-controller/transfer-memory implementation is still absent. No service
was replaced with unconditional success to increase the frame count.

The sampler fix is covered for all eight inactive comparison functions; enabling
comparison or anisotropy still fails. The cube was also run for three frames and
its capture visually inspected. It has no new source-derived pixel oracle.

The unmap fix removes all split views of the original mapping, preserves address
reservations and permits allocation release. Its next rendering rejection remains
explicit. Full raw diagnostic summaries are adjacent to this report.
