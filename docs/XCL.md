# Xbox Series X|S games through XCL (experimental)

`soju xcl` runs [XCL](https://github.com/xcl-dev/xcl), a compatibility layer that loads unpacked Xbox Series X|S
executables and stands in for the console's system libraries, on Soju's own engine and Apple's D3DMetal, in a bottle
of its own (`~/.battlenet-macos/xcl`). The Apple Silicon port is [MacBox](https://github.com/aykhanium/MacBox), which
uses CrossOver; Soju runs the same build without CrossOver.

**No game is playable yet.** MacBox reports GTA VI reaching its terms screen on an M5 Max with macOS 27.2, then a
crash after loading. Soju includes no game files and cannot get them: XCL needs your own already-unpacked copy of a
game you own.

```sh
soju xcl setup                       # XCL (about 2 MB) and Microsoft's shader compiler (32 MB), then the bottle
soju xcl run "/Volumes/Games/My Game"  # a game folder or its .exe; arguments for the game after --
soju xcl inspect "/Volumes/Games/My Game/game.exe"   # XCL's own tools: inspect, disasm, shaders, rpf
soju xcl status
soju xcl remove --yes
```

Saves, settings and logs go to the bottle's `%LOCALAPPDATA%\XCL\<title>\` (`logs\xcl.log` and `logs\report.json`
when something fails); the game folder is only read.

## What was checked (2026-11-25, M4 Pro, macOS 26.5, D3DMetal 3.0)

| Check | Result |
|---|---|
| `xcl --version`, `--help` | runs |
| Shader disassembly and RPF archive tests | 3 of 3 pass |
| GPU compute with wave64 and mesh shaders | 3 of 3 pass |
| Graphics draw (`gpu_graphics_vs_ps_draw`) | crashes |

The graphics test compiles its shaders and creates its pipeline, then the process dies inside Apple's D3DMetal once
the draw is submitted (SIGSEGV on a Metal thread, not a Windows exception). D3DMetal 4.0 beta 3 crashes the same way.
MacBox reports this test passing on macOS 27.2, so the difference is most likely Apple's Metal compiler in macOS 26;
macOS 26 also logged MTLCompilerService crashes on an atomic built-in during XCL's broader test run. Games draw with
the same path, so on macOS 26 they are expected to stop at their first frame.

## Where the pieces come from

- `xcl.exe`: MacBox at the commit pinned in `resources/xcl.json`, built unchanged with `scripts/build-xcl.sh`
  (GPL-2.0-only; the bundle carries the license and a NOTICE naming the source).
- `libunwind.dll`: LLVM-MinGW 20261117.
- `dxcompiler.dll` / `dxil.dll`: downloaded at setup from Microsoft's DirectXShaderCompiler release v1.9.2609
  (not redistributed by Soju), with its licenses.

Every download is pinned by size and SHA-256.
