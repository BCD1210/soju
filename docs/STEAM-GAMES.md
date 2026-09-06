# Steam + D3D11 games on Apple Silicon: the full free path

Verified 2026-08-27 on M4 Pro / macOS 26.5: Windows Steam client (Aug 2026 build,
CEF 126) renders and authenticates, and a Unity D3D11 title launched from the
library renders in-game, windowed, with a single Dock icon. Everything below is
free software.

## Why not the Soju CX engine?

The modern Steam client's CEF UI does not render on CrossOver-source builds:
the GPU process crash-loops (sandbox faults, 0xC0000005), and DXMT's
cross-process swapchain limit ([DXMT #141](https://github.com/3Shain/dxmt/issues/141))
blacks out the composer. The pinned upstream Wine 11 build works once the
steamwebhelper wrapper forces `--disable-gpu --single-process`
(from [notpop/steam-on-m1-wine](https://github.com/notpop/steam-on-m1-wine), MIT,
vendored in `third_party/`).

## The wiring that finally worked

Two D3D11 implementations must coexist in one prefix:

| consumer | needs | gets it via |
| --- | --- | --- |
| Steam client (32-bit steam.exe + 64-bit CEF helper) | vanilla wined3d | per-app `DllOverrides=native` → marker-stripped vanilla copies in `system32` |
| 64-bit games | DXMT (D3D11→Metal) | global `DllOverrides=builtin` → DXMT dlls installed as bundle builtins (x86_64 only) |

Hard-won facts, in the order they burned us:

1. **DXMT dlls must be builtins.** Loaded as native they cannot attach their
   unixlib (`winemetal.so`). Wine falls back to vanilla silently.
2. **Wine-built PEs dropped into a game folder are treated as fake dlls** and
   silently redirected to the bundle builtin. Game-local DXMT does not work.
3. **A builtin that isn't part of wine needs a placeholder**: without a
   `winemetal.dll` copy in `system32`, by-name lookup fails with `c0000135`
   ("Failed to initialize graphics" in Unity) even though the builtin exists.
4. **The Steam client dies on DXMT builtins** (helper restart loop every 10 s),
   so it must be forced to vanilla per-app. But per-app `native` pointing at a
   vanilla wine PE gets redirected to the (DXMT) builtin, unless you strip the
   `Wine builtin DLL` marker from the copies (1-byte patch).
5. **i386 stays vanilla.** steam.exe is 32-bit; its composer must not see DXMT.
6. **`winemac.so` must export macdrv symbols** (`-fvisibility=default` rebuild)
   or DXMT's `_CreateMetalViewFromHWND` cannot dlsym them.
7. **Env `WINEDLLOVERRIDES` beats per-app registry**: keep d3d overrides out of
   the env; drive the split from the registry only.
8. **Steam tags games with `DISABLEDXMAXIMIZEDWINDOWEDMODE`**, forcing
   fullscreen; scrub it from `user.reg` for windowed play. Unity then remembers
   `-screen-fullscreen 0`.
9. **Crashed Chromium leaves `SingletonLock`** in htmlcache; the next launch
   silently becomes `--silent` (no window). Purge on every launch (play.sh does).
10. **One Dock icon**: macdrv registers a Dock icon per wine process with
   windows, and this macdrv ignores virtual desktops. We patched
   `cocoa_app.m` (modeled on CrossOver's hack 24141) to honor
   `WINE_NO_DOCK_ICON="steam.exe;steamservice.exe"`, matching the basename of
   the first two argv entries (NSProcessInfo sees pre-rewrite argv; scanning all
   args would also hide the helper via its `-steampath=` argument).

## Automatic setup

`soju steam-install` now downloads verified prebuilt DXMT, the patched Wine driver and wrapper. Existing users can close Steam and run `soju steam-games`. A private Wine runtime preserves the Homebrew app. This component release requires macOS 26+ and Wine 11.0. See [sources and validation](STEAM-PREBUILTS.md).

## Building the artifacts

Based on notpop's `07-build-dxmt-fork.sh` / `08-patch-wine-visibility.sh`, with
three fixes we needed on macOS 26.5 / current Homebrew:

- meson must be 1.10.2 (`python3 -m venv … && pip install 'meson==1.10.2'`,
  pass `MESON=`), Homebrew's 1.12 breaks the DXMT build.
- `src/util/com/com_guid.cpp` needs `#include <iomanip>` (newer mingw).
- The prebuilt LLVM 15 x86_64 tree references zstd: build an x86_64
  `libzstd.a` and `libtool -static`-merge it into `libLLVMSupport.a`.
- The 3Shain wine toolchain tarball extracts flat. Normalise into
  `toolchains/wine/`.
- The Dock-icon patch on top of the visibility rebuild lives in
  `transformProcessToForeground:` (see `scripts/setup-steam-games.sh` header).

The pinned DXMT fork is MIT (Copyright Feifan He), with separate notices for bundled DirectX headers; the wrapper is MIT
(vendored in `third_party/`); our winemac patch is published as
`patches/winemac-no-dock-icon.patch` (LGPL-2.1+, matching Wine). The same patch adds
`WINE_DOCK_REOPEN_CMD`: when the Dock icon is clicked and no Wine window is visible, winemac runs
the command, `play.sh steam` sets it to `steam.exe steam://open/main`, so closing the Steam window
parks it (as on Windows) and a Dock click brings it back; Steam > Exit really quits. Nothing
proprietary is redistributed.

Artifacts land in `~/.battlenet-macos/steam-support/` and
`scripts/setup-steam-games.sh` wires everything idempotently.

## Runbook

```bash
scripts/create-steam-bottle.sh    # private Wine 11 + wrapper + Steam
scripts/setup-steam-games.sh      # verified download + private DXMT/winemac runtime
scripts/play.sh steam             # play
```


### Repairing and removing Steam

Run `soju steam-install` again to repair the Wine runtime for an existing Steam
installation. Existing games remain in place. Close Windows Steam and its games
before repairing or changing the renderer.

`soju uninstall` resolves the same Steam runtime used for launching, waits for the
bottle to stop, and aborts if that fails. The separate Steam runtime category
includes the downloaded Wine build, prepared renderer and their rollback copies.
Declining bottle removal keeps installed games; separately installed Wine apps and
external `SOJU_STEAM_WINE` directories are preserved.
