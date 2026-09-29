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

## Per-game renderer

DXMT remains the default. For a **64-bit Steam game** with a renderer-specific
problem, the CLI can select DXVK + MoltenVK for just that executable. In [#48](https://github.com/BCD1210/soju/issues/48), a Dark Souls III player reports that this avoids minute-long GPU stalls, with a lower, CPU-limited frame rate of about 30 FPS. This is a user-tested workaround, not a claim that the DXMT fault is fixed.

Close Windows Steam and all its games first. Use the full **Mac path** to the game
executable; for a default Dark Souls III install:

```bash
GAME="$HOME/.battlenet-macos/steam-bottle/drive_c/Program Files (x86)/Steam/steamapps/common/DARK SOULS III/Game/DarkSoulsIII.exe"
soju steam-games renderer dxvk "$GAME"
```

Soju downloads the pinned [Gcenx DXVK-macOS native package](https://github.com/Gcenx/DXVK-macOS/releases/tag/v1.10.3-20230507-repack), checks the archive and DLL SHA-256 digests, backs up existing game-local DLLs and overrides, then installs only `d3d11.dll` and `d3d10core.dll` next to the game. The executable's `dxgi=native` uses Soju's existing vanilla Wine DXGI; `winemetal` is disabled for this executable. No DLLs are replaced in the shared Wine runtime or Steam client. `dxvk.conf` and async settings are left unchanged.

```bash
soju steam-games renderer status "$GAME"  # inspect without changing anything
soju steam-games renderer dxmt "$GAME"    # explicitly use Soju's DXMT
soju steam-games renderer reset "$GAME"   # restore the original files and overrides
```

Restart Steam and the game after a change. Renderer setup/repair writes only the
global defaults and Steam client profiles, so game-specific overrides remain.
This command preserves previous settings, including a manually installed
workaround: `reset` returns to that original configuration, whereas `dxmt`
explicitly selects DXMT. Wine scopes overrides by executable name, so another
game with the same executable name in this bottle cannot have a separate profile.
Executables sharing a directory also share the local DLLs and cannot have competing
profiles. If a game updater or another tool changes the managed DLLs/overrides,
Soju refuses to overwrite those changes; recovery copies are kept under the
Steam bottle's `.soju-renderers` directory.

The upstream DXMT tessellation change mentioned in #48 has **not** been applied:
the reporter also found a Wine driver deadlock with upstream main, so replacing
the pinned runtime needs separate compatibility testing.

### Renderer validation

On 2026-09-28, the D3D11 hardware smoke test passed on M4 Pro / macOS 26.5 in a temporary prefix: default DXMT, per-game DXVK, DXVK after a full `--repair`, explicit DXMT, and reset to the original configuration. Each pass created a hardware device and swapchain and presented 180 frames. No installed game files were used. This verifies the switch and repair behavior; Dark Souls III gameplay and performance still require confirmation from the reporter.

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
