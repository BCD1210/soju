# D2R-PCVR front-end controller fix

Upstream: https://github.com/bearstonem/d2r-pcvr
Base: `70ad0e665952f1bd829176ba461a85ebb974c3f0` (v0.3.31).
License: Mozilla Public License 2.0; see LICENSE here and the upstream notices.
`vr-character-create.patch` contains all Soju source changes against that base,
including the portable regression test and macOS cross-build include fix.

The anchored front end accepts native controller input without desktop focus.
It does not pull focus from the simulator, and mouse injection still requires
D2R focus. The existing Native-tagged BodyWalk/XInput path selects classes.
On OXRSys, a fresh title-screen A/Start posts Enter to the game window only while
TitleScreenPanel is open. A release gate prevents that press carrying into Play.
A black projection background makes OXRSys stream quad-only menus. During loading,
the in-process virtual pad returns neutral controls instead of disconnecting;
external BodyWalk writers retain their timeout.

The UI thread restores native controller layout on the front end. A VR Play/New
strip maps stick-right/A to native Start, and the name screen has a stick/A keyboard
with X erase, Y case and native B back. Screen changes consume held buttons.
Keyboard events use the game's window thread and TranslateMessage with scoped
modifier state; they never activate the desktop window. Physical typing stays on
the original input path. Front-end, native-panel, composition and atlas tests cover
the portable state and layout rules; name entry is also verified live.

## Rebuild

Use local copies of the upstream source and SDK checkouts. The script archives
only the pinned base, applies this patch in a temporary directory, verifies SDK
commits and tracked cleanliness, and builds without downloading or installing.
Use llvm-mingw-20260616-ucrt-macos-universal, CMake, Ninja and Python 3.9+.
The SDK commits are pinned in the script and upstream setup_windows_tools.ps1.

```sh
python3 scripts/build-vr-mod.py \
  --source /path/to/d2r-pcvr-src \
  --deps /path/to/pcvr-0331/deps \
  --toolchain /path/to/llvm-mingw-20260616-ucrt-macos-universal
```

Output: `dist/vr-mod/d2rl-pcvr-soju.dll`, a complete patched source archive,
MPL license, and patch. The build prints sizes and SHA-256 hashes. Byte-for-byte
reproducibility across paths/toolchain versions is not assumed; pin the exact
built DLL that will be published. No game assets are included.

## Release gate

No artifact has been uploaded by this change. After simulator qualification (real Quest remains unverified):

1. Publish the staged DLL at the `vr-mod-v1` URL in resources/d2r-vr.json. Confirm
   its size and SHA-256 match `mod_patch`; never replace a published artifact.
2. Publish `d2r-pcvr-soju-source.tar.xz`, LICENSE, and the patch alongside it.
   Keep the source available for recipients under MPL-2.0, and publish this repo's
   patch and rebuild script with the Soju release. Preserve upstream notices.
3. If rebuilding changes the bytes, update the manifest to the new exact size
   and digest before release. All setup downloads use the existing verified
   downloader; none bypass checksum validation.
4. Smoke-test fresh setup and upgrade, then distribute Soju. Do not distribute
   this manifest while the pinned release asset is absent.

For the current local test, only `mods/pcvr-dev/d2rloader/plugins/d2rl-pcvr.dll`
is replaced, with a verified backup. The installed release mod stays untouched.
