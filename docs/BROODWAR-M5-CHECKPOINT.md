# M5 checkpoint

This checkpoint does not meet Remastered visual parity or full controller acceptance.
All captures, extracted art, private maps, and detailed logs remain outside Git at
`~/.local/share/soju/broodwar-native/3d-proof/m5/`.

## Changes and evidence

- `3697e78`: preserve prelit terrain, clamp fog sampling, add orthographic capture controls and fixture terrain preservation checks.
- `e7758b1`: correct the vertical orientation of direct DDS terrain textures. The live fixtures retain Fading Realm's DIM, ERA, MTXM, THG2 and DD2 sections byte-for-byte; see `terrain/live-provenance.json`. Earlier v2 fixture startup failed; successful v4 uses the established acceptance scenario layout with the owned terrain sections.
- `9adb44b`: missing model imports fall back to owned HD image chains, native directional frames and mirror flags. Types 114, 116, 123 and 135 were visible in desktop and simulator captures. Debug overlays default off; backtick toggles them. Catalog audit covers 226 drawable main-image mappings; types 101 and 214 are unused/start-marker exceptions. Camera-facing sprites retain the native game-space direction rather than selecting a new direction relative to the viewer.
- `ebb9065`: extract 37 owned HD creep border frames per tileset with eight-neighbor lookup, render borders and interiors, expose 24 targeted spells using native orders/tech energy metadata, extend race command cards, and record controller diagnostics. Storm, Dark Swarm and Defensive Matrix passed live order/energy checks. Wrong-caster and unknown-action requests were rejected. Desktop spell cycling uses C, right-click dispatch and Escape cancellation.
- `2341618`: use float trigger/grip actions for Touch controllers with press/release hysteresis; retain boolean selection for simple controllers.
- `b3bb269`: isolate the attribution test's temporary repository with `core.hooksPath = os.devnull`.

## Captures and comparison

| Evidence | Runtime path |
| --- | --- |
| Terrain before/after/source comparison | `terrain/comparison.png` and `comparison.json` |
| Corrected desktop terrain | `terrain/uv-check/` |
| Missing-model desktop and XR frames | `fallback/desktop/`, `fallback/xr/` |
| Missing-model simulator | `fallback/simulator.png` |
| Live cast results | `commands/spell-results.json` |
| Corrected creep desktop and XR frames | `commands/creep-desktop-fixed/`, `commands/creep-xr-fixed/` |
| Creep simulator | `commands/creep-simulator.png` |
| Controller attempt frames and diagnostics | `gestures/` |

The terrain reference is owned HD tiles assembled in map order, **not a matched
HD client screenshot**. The 20,086-pixel comparison excludes unit bounds. SSIM
improved from 0.675815 to 0.941860; PSNR from 29.8069 to 35.8877 dB; mean absolute
channel error from 4.82346 to 1.75940. `terrain/remastered-client.png` captures the
original client but appears to use classic graphics and is not the HD benchmark.

## Validation

- Asset-venv Python: 245/245 tests passed (`full-suite.log`).
- Attribution tests: 10/10 passed with a deliberately failing global pre-commit hook (`global-hooks-test.log`).
- Desktop and XR renderer checks: 43 each passed (`commands/runtime-checks.json`, `commands/xr-runtime-checks.json`).
- Native Rust tests: 13 passed; Miri: 13 passed (`commands/miri.log`). The final targeted-packet byte assertion was also rerun under ordinary Rust tests.
- Desktop and OpenXR simulator both displayed live terrain, fallback sprites and creep. Streaming frames alone do not establish gesture acceptance.

## Remaining acceptance gaps

1. Oblique terrain views still expose seams/stretching around raised terrain. HD water tiles are static. Cliff/ramp geometry, water animation and doodad placement need matched client-frame acceptance, not just tile-source agreement.
2. Existing 3D models still show coarse geometry and white material patches. Billboard fallback prevents missing units but does not establish Remastered-equal model quality or view-relative directional correctness for every type.
3. Sun direction and shadow softness are not calibrated. The earlier shadow fit was weak (R-squared about 0.089), so it was not treated as a valid measurement. Creep contours use source frames but interior variation does not reproduce the native random sequence; no matched creep score is claimed.
4. Command cards are expanded, not exhaustive for all units/orders. Hero variants, special production/merge commands and prerequisite-aware presentation remain. Only three of the 24 targeted spells have live cast evidence. Targeting-ray acceptance remains incomplete.
5. Simulator trigger/grip key attempts produced no observed OpenXR input transitions. Box-select, double-tap and grab/zoom are **not accepted**. Input telemetry and analog bindings are available for diagnosing the next attempt; no synthetic input is counted as acceptance.
6. A matched owned Remastered HD client frame for the same camera/area is still needed for the requested final visual score.

No assets were added to Git, `asset-upgrades/` was not modified, and nothing was
pushed. Only the dedicated bottle/viewers were restarted. Process checks found no
running `devplay.py`; no D2R process was stopped.
