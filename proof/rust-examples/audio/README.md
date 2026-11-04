# audren PCM acceptance

`bash scripts/test-switch-audio.sh` runs the original local `audio__audren-simple.nro`,
presses A after its first presentation, pumps the existing CoreAudio path, requires
at least 2,000 nonzero samples consumed by the native AudioUnit callback, then
presses PLUS and requires a clean guest exit. The recorded run consumed 2,592
nonzero samples after 14 presented frames. The synthetic audout regression also
passed with 47,920 nonzero samples. This measures callback consumption, not an
acoustic recording.

The renderer implements manager work-size/open, retained transfer-memory lifetime,
renderer state/start/stop/event, and both update command IDs. Update layout follows
the public libnx audren structures. The supported profile is 48 kHz PCM16, 240
samples per 5 ms period, up to 64 voices, mono/stereo voices, one stereo device
sink, and the final mix. Voice and master volume, channel routing, saturation,
looping, pause, buffer consumption and played-sample feedback are implemented.
The existing host mixer delivers the resulting PCM to CoreAudio.

Input section lengths, attached memory pools, sample ranges, gains, wave byte
budgets and output buffers are checked before publishing an update. Renderer
wave snapshots are limited to 8 MiB, host packets to 200 periods, and catch-up
to eight periods per poll. Unit checks cover routing, consumption exactly once,
looping, pause, clipping, invalid wave ranges, format rejection and atomic rejection
of malformed updates.

Remaining audio gaps: 32 kHz output, resampling/pitch, ADPCM, effects, filters,
submixes/splitters, more than two channels, circular sinks, performance telemetry
and later revision features. Unsupported configurations stop explicitly.

Public ABI references:
- https://github.com/switchbrew/libnx/blob/master/nx/include/switch/services/audren.h
- https://github.com/switchbrew/libnx/blob/master/nx/source/services/audren.c
- https://github.com/switchbrew/libnx/blob/master/nx/source/audio/driver.c
- https://github.com/switchbrew/libnx/blob/master/nx/source/audio/voice.c

Local input hashes (assets remain outside the repository):
- `audio__audren-simple.nro`: `1968054878b2d5dce907bb78555d15fe23d1f7e7439c2d6309238cceeb0f7b90`
- `main.c`: `ba62bbb19a76e6b627ad6b50ba28f9ae04962d8634d6534b83be4963170f4adb`
- `audren.h`: `d1258027beb7f2533343346f3247091ff6407c8cc4bca97a9ab4ef8aa65169f4`
