# Wine patches from Highball

Taken unchanged from [Highball](https://github.com/gauthierpiarrette/highball-engine) (`patches/`, LGPL-2.1,
commit 34bd3b09, 2026-11-04). `scripts/build-engine.sh` applies them in file order, after Soju's own patches.
Highball's README there explains each one. The fourteen kept here:

| Patch | What it fixes |
| --- | --- |
| 0005 | A job hand-off that signals an event twice loses one under msync |
| 0010 | Unity and Unreal in-game videos stay blank |
| 0014 | The wineserver grows until a long session stalls (msync) |
| 0015 | Short audio dropouts at 48 kHz |
| 0017, 0018 | Forza Horizon 6 crash at start, and programs that warn of low memory |
| 0019 | Xbox games ask for a sign-in at every launch |
| 0020 | Retina mode reports twice the screen size to games |
| 0022, 0023 | Surround (5.1, 7.1) audio on Mac speakers and headphones |
| 0024 | Games that wait for the VCLibs framework packages |
| 0026 | Rockstar sign-in refused in UTC+3 time zones |
| 0028 + 0028b | A fault on a Metal / D3DMetal thread hangs the game at full CPU; it now ends the program with a real crash report |
| 0029 | concrt140 / msvcp90 concurrent_vector fixes from upstream Wine |

`0028b` is Soju's: 0028 alone made Diablo II: Resurrected hang, because D3DMetal's threads also hit the
page faults Wine resolves on its own threads (Rosetta reporting writes as reads, pages Wine toggles between
writable and executable, write watches). 0028b resolves those first, without a TEB, and lets 0028 end the
program only for what is left.

Not taken: 0001-0003 (Highball's own layout), 0006 (not needed), 0007 (Soju keeps it as
`patches/winemac-cross-process-swapchains.patch`), 0011, 0012 and 0016 (do not apply to this tree or need Highball's x87 sidecar).
Also left out after testing on this engine (M4 Pro, macOS 26.5, Diablo II: Resurrected under D3DMetal,
2026-11-11), each added on top of patches that let the game start:
- 0008 + 0009 (fiber pointer in %gs:0x20): the game hangs on a lock at start.
- 0013, 0025, 0027 (each changes ntdll's Windows-side code: DEP policy, threadpool, `.local` lookup): the game dies
  at start (stack overflow or an exception it cannot dispatch), also with `WINE_DISABLE_NX_COMPAT=0` for 0013.
