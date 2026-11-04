# Soju Add game → Play

Verified on 2026-11-04 with the desktop app built from this checkout. The local
es2gears NRO was imported through Platforms → Nintendo Switch → Add game,
then selected in Library and launched with Play. The app's bundled switch.sh
selects the installed Rust runner automatically. No GPU environment override
was set. The older running app bundle still invoked the Python runner; the
current app bundle was built at dist/app-launch-final/Soju.app for verification.

The runner is installed and ad-hoc signed at
~/.local/share/soju/switch/soju-switch-core. SDL explicitly selects Metal.
The native host now registers as a regular macOS application and raises its
window at launch and first presentation, so launching from the parent app does
not leave the game behind other windows.

app-and-game.png is a real desktop screenshot showing the Soju Library entry
marked Running and the foreground game window. game-window.png and
soju-library.png are separate native window captures from the same launch flow.
No compositing or generated visuals are used. The rotating gears were observed
at multiple poses; screenshots are display proof, while the source-derived
headless pixel oracle remains in core/tests/gpu_gears.rs.

The app packager now excludes every Rust target directory before copying
resources (previously it copied all of them, then removed only core/target).
Verified bundle size: 5.5 MB; no target directories; codesign strict verification
passed both before and after Play. Metal archives now default to
~/Library/Caches/Soju/Metal/v1; the old cwd-relative cache invalidated the app
bundle seal. SOJU_GPU_CACHE_DIR still overrides the location.
The verified bundle was then installed at ~/Applications/Soju.app. The previous
bundle was preserved as a timestamped Soju.previous-switch-*.app sibling.
Other running app bundles were not replaced. Build artifacts stay under ignored dist/.

Installed runner SHA-256 at capture:
7d438f3dd4544418d0558d1adb510c02657b814af9053af91edb56bca384af12

Observed process chain: Soju PID 51445 → installed runner PID 53885.
