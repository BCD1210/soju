# Per-title results

Base `989b89c`: **44 frames / 14 stops / 1 clean exit**. Current runtime `98da0ac` (unchanged by test-only `7543d99`): **58 / 0 / 1** in one complete quiet run.

All 59 processes returned status 0 within the 15-second per-title budget. No concurrent builds/tests from this session ran during the final corpus run. The earlier full run overlapped core tests and recorded two console timeouts; it is retained as [initial results](corpus-initial.txt), not used for the final count. Both consoles also passed separate quiet retries.

Public local devkitPro examples; fresh home per title; default Metal backend. First-presentation evidence does not establish full scene/service coverage.

| Title | Before | After |
| --- | --- | --- |
| account | frame | frame |
| alarm-notifications | stop | frame |
| app_controldata | stop | frame |
| applet__app-playstats | stop | frame |
| applet__libapplets__error | frame | frame |
| applet__libapplets__pctlauth | frame | frame |
| applet__libapplets__software-keyboard | frame | frame |
| applet__libapplets__web | frame | frame |
| applet__light-sensor | frame | frame |
| applet__lockexit | frame | frame |
| applet__recording | frame | frame |
| applet__screenshot-overlay | frame | frame |
| applet__vrmode | frame | frame |
| audio__audren-simple | frame | frame |
| audio__echo | stop | frame |
| audio__playtone | frame | frame |
| exception-handler | frame | frame |
| fs__romfs | frame | frame |
| fs__save | frame | frame |
| fs__sdmc | frame | frame |
| graphics__deko3d__deko_basic | frame | frame |
| graphics__deko3d__deko_console | stop | frame |
| graphics__deko3d__deko_examples | frame | frame |
| graphics__opengl__dynamic_resolution | frame | frame |
| graphics__opengl__es2gears | frame | frame |
| graphics__opengl__gpu_console | stop | frame |
| graphics__opengl__lenny | frame | frame |
| graphics__opengl__simple_triangle | frame | frame |
| graphics__opengl__textured_cube | frame | frame |
| graphics__printing__hello-world | frame | frame |
| graphics__printing__vt52-demo | frame | frame |
| graphics__sdl2__sdl2-simple | frame | frame |
| graphics__simplegfx | frame | frame |
| graphics__simplegfx_moviemaker | exit0 | exit0 |
| hid__abstracted-pad | stop | frame |
| hid__colors | frame | frame |
| hid__hdls | stop | frame |
| hid__irsensor | stop | frame |
| hid__notification-led | stop | frame |
| hid__read-controls | frame | frame |
| hid__ring-con | frame | frame |
| hid__sevensixaxis | stop | frame |
| hid__sixaxis | frame | frame |
| hid__touch-screen | frame | frame |
| hid__vibration | frame | frame |
| jit | frame | frame |
| misc__barrier | frame | frame |
| misc__user_events | frame | frame |
| misc__user_timers | frame | frame |
| network__curl | frame | frame |
| network__ldn | stop | frame |
| network__lp2p | stop | frame |
| network__nxlink_stdio | frame | frame |
| nfc | frame | frame |
| power | frame | frame |
| settings__get_system_language | frame | frame |
| templates__application | frame | frame |
| time | frame | frame |
| usb__usbhs | stop | frame |
