# Steam support release v1

The package contains only these open-source components:

- DXMT: notpop/dxmt commit `924a607e3eee06fad5be6f176d8510bb08bc418d`,
  plus `patches/steam-dxmt-build.patch` (MIT; bundled DirectX headers retain their own notices).
- Wine 11.0 winemac: `db11d0fe6a169c457e23d007e20404643d067aa8`,
  plus `patches/steam-winemac.patch`, built with public driver symbols
  (LGPL-2.1-or-later).
- Steam webhelper wrapper: vendored notpop source (MIT).
- LLVM 15.0.7: `8dfdcc7b7bf66834a761bd8de445840ef68e4d1a`
  (Apache-2.0 with LLVM exceptions), statically linked into the renderer.

[Binary and corresponding source release](https://github.com/BCD1210/soju/releases/tag/steam-support-v1)

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
