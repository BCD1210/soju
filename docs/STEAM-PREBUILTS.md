# Steam support release v9

The package contains only these open-source components:

- DXMT: notpop/dxmt commit `924a607e3eee06fad5be6f176d8510bb08bc418d`,
  plus `patches/steam-dxmt-build.patch` and `patches/dxmt-max-tess-factor.patch`
  (a backport of upstream DXMT ef116534, the `d3d11.maxTessFactor` option)
  and `patches/dxmt-nonfatal-pure-virtual.patch` (a backport of upstream DXMT 66277b82e: a call on a
  destroyed object, which Skyrim Special Edition makes at startup, is logged instead of aborting the game)
  (MIT; bundled DirectX headers retain their own notices).
- Wine 11.0 winemac: `db11d0fe6a169c457e23d007e20404643d067aa8`,
  plus `patches/steam-winemac.patch`, built with public driver symbols
  (LGPL-2.1-or-later).
- Wine 11.0 winecoreaudio (Unix side) and x86_64 mmdevapi, same revision, with
  `patches/steam-wine-audio.patch`: a 5.1/7.1 stream on a stereo device is folded to stereo, so centre
  and back channels are heard, and the device's I/O buffer stays under half a period (#93)
  (LGPL-2.1-or-later).
- Wine 11.0 x86_64 wow64cpu, same revision, with `patches/wow64cpu-rosetta2-far-jump.patch`, and winemac
  also with `patches/winemac-unsafe-display-modes.patch` (both rebased from CrossOver 26, LGPL-2.1-or-later):
  32-bit games return from system calls with `lretq` under Rosetta 2, and low fullscreen modes such as
  640x480 can be selected (#153).
- Wine 11.0 opengl32 (i386 PE and Unix side), same revision, with `patches/opengl32-wow64-batch.patch`
  (Soju's, LGPL-2.1-or-later): a 32-bit game's immediate-mode OpenGL calls cross to 64-bit code in
  batches instead of one by one (#153).
- Steam webhelper wrapper: vendored notpop source (MIT).
- MoltenVK v1.4.2: `db66022459ffb663aa2b50f6b018bc2e124f5edf` (Apache-2.0), x86_64, with
  `patches/moltenvk/zero-missing-vertex-attributes.patch`, built by `scripts/build-moltenvk.sh`. It replaces the
  Steam runtime's MoltenVK 1.4.1, which could not compile Detroit: Become Human's shaders
  (`DrawIndex is not supported in MSL`, fixed in 1.4.2). The patch lets a pipeline build when its vertex shader
  reads a location the vertex input state does not describe (Vulkan allows it, Metal refuses the pipeline):
  those inputs read zero. Steam starts with `MVK_CONFIG_SHOULD_MAXIMIZE_CONCURRENT_COMPILATION=1` and
  `MVK_CONFIG_SHADER_COMPRESSION_ALGORITHM=1`.
- LLVM 15.0.7: `8dfdcc7b7bf66834a761bd8de445840ef68e4d1a`
  (Apache-2.0 with LLVM exceptions), statically linked into the renderer.

[Binary and corresponding source release](https://github.com/BCD1210/soju/releases/tag/steam-support-v9)
([v8](https://github.com/BCD1210/soju/releases/tag/steam-support-v8) is the same without the opengl32 patch; [v7](https://github.com/BCD1210/soju/releases/tag/steam-support-v7) is the same without the wow64cpu and display-mode patches; [v4](https://github.com/BCD1210/soju/releases/tag/steam-support-v4) is the same without MoltenVK; [v3](https://github.com/BCD1210/soju/releases/tag/steam-support-v3) is the same without the audio components;
[v2](https://github.com/BCD1210/soju/releases/tag/steam-support-v2) is the same without the pure virtual
backport; [v1](https://github.com/BCD1210/soju/releases/tag/steam-support-v1) also lacks the tessellation backport.)

No Apple GPTK, game files, store clients, accounts or credentials are included.
Clients are downloaded from their official installers separately. The manifest
in `resources/steam-support.json` pins both the archive and every extracted file
with SHA-256. Links, traversal, duplicate files and unexpected payloads are
rejected before installation.

## Build

On Apple Silicon with Xcode's Metal toolchain and Homebrew build tools:

```bash
brew install cmake ninja bison flex gettext mingw-w64 python
SOJU_STEAM_BUILD="$PWD/steam-build" bash scripts/build-steam-support.sh
```

The script pins Meson 1.10.2, builds x86_64 LLVM with zstd disabled, applies the
published patches, and stages binaries without touching installed Wine or games.
The source release includes exact DXMT, Wine, LLVM and DirectX-header snapshots,
the Wine 8.16 toolchain source, licenses and a rebuild script.

## Validation

On M4 Pro / macOS 26.5: Wine 11.0 private-runtime migration, registry setup,
D3D11 hardware device/swapchain creation and 180 presented frames using the
DXMT winemetal module. The smoke test source is `tests/steam_d3d11_smoke.c`.
The current native component deployment target is macOS 26.0.

v2, on M4 Pro / macOS 26.5 in a throwaway prefix: the smoke test again presented
180 frames. `tests/steam_d3d11_tessellation.c` draws 40,000 patches per frame
whose hull shader requests factor 64. It averaged about 124 ms per frame
without a cap (the same as v1), 12.3 ms with `d3d11.maxTessFactor = 16` and
5.0 ms with `8`. v1 ignores the option.

v3, on M4 Pro / macOS 26.5: only `dxmt-x64/d3d11.dll` differs from v2 (the other files
are v2's, byte for byte; a rebuild changes only their PE timestamps). In it, the
`ud2` trap after "Pure virtual function called" is replaced by a return. Installed
through `soju update` from the published release, a Unity D3D11 game (Guilt Free)
reached its main menu. Skyrim Special Edition's startup abort was checked by the
contributor of #92 with the same instruction change.

v4, on M4 Pro / macOS 26.5: only `winecoreaudio-patched.so`, `mmdevapi-x64.dll` and `SOURCES.txt`
are new; every other file is v3's, byte for byte. Two test programs ran in the Steam runtime with the
new files: a WASAPI 5.1 float stream (mask 0x3f) with a tone in one channel at a time, and Skyrim's
shape, a 6-channel XAudio2 mastering voice at 44.1 kHz with a mono voice sent only to the centre. A
meter on the device unit's render callback measured what reached the 2-channel built-in speakers.
With v3 the device unit was handed all 6 channels and the tone sat in channel 3, which Core Audio
does not play on stereo speakers (as measured in #93). With v4 the log shows
`6-channel stream (mask 0x3f) rendered by AUSpatialMixer for built-in speakers on a 2-channel device`
and the centre tone reached both speakers (RMS 0.025 each for a 0.035 input); back left and back right
went mostly to their side. The Overwatch/Diablo II Steam mode uses the CX engine, which already has
these patches.

v8, on M4 Pro / macOS 26: only `winemac-patched.so`, `SOURCES.txt` and the new `wow64cpu-x64.dll` differ
from v7 (a rebuild of `winecoreaudio-patched.so` from the same tree matched v7 byte for byte). Sven Co-op
(free on Steam, GoldSrc `hw.dll`/`vgui2.dll` like Counter-Strike 1.6) loading the map `crystal` crashed
twice in a row on v7 with `page fault on read access to 0x00004ecd ... at address 7BE01139`, the address
in #153: `syscall_32to64`'s `movl cs32_sel(%rip),%edx` decoded as 32-bit code. With v8 it reached gameplay
from the command line and from Steam's Play button. Fullscreen at 640x480 showed only black on v7 and the
scaled menu on v8; the desktop mode came back after the game quit. The Steam client (32-bit) still signed in
and installed games. Counter-Strike 1.6 itself was not run (not owned on the test account).

v9, on M4 Pro / macOS 26: v8 plus `opengl32-i386.dll` and `opengl32-unix.so`; every other file is v8's.
Sven Co-op (GoldSrc, like Counter-Strike 1.6), map `crystal`, same spot, frames counted from buffer swaps
over 10 s while the Mac was busy with other work (load 15 to 60, so the absolute numbers are low):

| | 640x480 window | 2560-wide window |
|---|---|---|
| v8 | 10 fps | 7 fps |
| v8, game started with `-nomtex` | 22 fps | |
| v9 with `scripts/gl-app-defaults.sh` (no launch option) | 30 fps | 36 to 39 fps |

A trace of one v8 frame had 45,509 OpenGL calls, 880 of them `glTexSubImage2D` on 128-wide lightmap rows
between draws; a sample showed the time in Apple's `gldModifyTexSubImage`, `flushContext` and
`beginRenderPass`. Without `GL_ARB_multitexture` the engine uploads each lightmap page once per frame.
Counter-Strike 1.6 itself was not run (not owned on the test account).
