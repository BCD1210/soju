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
| XCL's synthetic console title (opens its window, draws, presents) | runs to the end: 47 checks pass, 3 fail |

The synthetic title renders a triangle on the GPU and reads it back, creates its swap chain and shows its first
frame. Its three failures are the same kinds MacBox lists as open on CrossOver: the console window is not cloaked
(Wine's DWM does not implement cloaking, so it may show as an empty window), no GPU timestamps, and XMA2 audio.

One of XCL's unit tests, `gpu_graphics_vs_ps_draw`, crashes inside Apple's D3DMetal on this Mac: it draws
offscreen with no window at all, even with two-line shaders, and the process dies once the draw is submitted
(D3DMetal 3.0 and 4.0 beta 3 alike; MacBox reports it passing on macOS 27.2). The same draw with a window, as a game
does it, works.

## Where the pieces come from

- `xcl.exe`: MacBox at the commit pinned in `resources/xcl.json`, built unchanged with `scripts/build-xcl.sh`
  (GPL-2.0-only; the bundle carries the license and a NOTICE naming the source).
- `libunwind.dll`: LLVM-MinGW 20261117.
- `dxcompiler.dll` / `dxil.dll`: downloaded at setup from Microsoft's DirectXShaderCompiler release v1.9.2609
  (not redistributed by Soju), with its licenses.

Every download is pinned by size and SHA-256.
