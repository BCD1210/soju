# Steam support release v6

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

[Binary and corresponding source release](https://github.com/BCD1210/soju/releases/tag/steam-support-v6)
([v4](https://github.com/BCD1210/soju/releases/tag/steam-support-v4) is the same without MoltenVK; [v3](https://github.com/BCD1210/soju/releases/tag/steam-support-v3) is the same without the audio components;
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
