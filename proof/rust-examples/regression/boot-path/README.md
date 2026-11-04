# Self-built application boot path

This change prepares and verifies a bounded application boot path. Commercial
game boot compatibility is **not established**, and the requested rendering
expansion is **not complete**. No owner-supplied commercial dumps were available.

| Probe | Checks passed | Scope |
| --- | ---: | --- |
| application | 18 | Application proxy, focus, operation/performance mode, desired language, preselected user, profile, consumed launch parameter, control property |
| input | 18 | Npad style roundtrip, handheld/dual setup, vibration send, six-axis start/sample/stop |
| platform | 15 | Language/region, firmware-version ABI, user/network clocks, mapped host font |
| offline | 17 | NIFM offline query, local TLS context creation/release, friends factory, parental restrictions, prepo transmission status |
| storage | 24 | Save write/read/commit, previous commit, BCAT/cache mounts, RomFS file, container control property |
| media | 24 + 2 pixel checks | Docked mode, nvdrv/vi layer/buffer queue, 1280×720 and 1920×1080, audren initialization/update/start/stop |
| encrypted NSP, launch 1 | 24 | PFS0 → encrypted Program/Control NCA → NSO → own rtld → libnx main |
| encrypted NSP, launch 2 | 24 | Same path; reads the first process's committed save before writing |

Each adjacent text file contains the per-call outcomes. Rerunning against a new
empty save home changes the number of create/read-existing checks, not the
acceptance requirement. All probes must emit a total, contain no FAIL, and exit
normally. Media also requires exactly two presentations and a byte-for-byte
pixel match. The [720p](1280x720.png) and [1080p](1920x1080.png) captures contain
only the self-built test pattern. This is buffer-queue acceptance, not a shader
rasterization acceptance.

119 core library tests pass with default Metal and again with the CPU backend
selected. Four Program-loader tests pass, including control-property size and
duplicate rejection. All ten existing source-derived scene acceptances pass.
See `core-tests.txt`, `core-off-tests.txt` and `accepted-suite.txt`. The normal
headless CLI also exits successfully on the synthetic NSP (`retail-headless.txt`).

Both default corpus runs report **42 frames / 16 stops / 1 clean exit**
against the supplied **40/18/1** baseline. `applet__vrmode` and `hid__sixaxis`
change from stop to first presentation. First presentation does not establish
complete applet/controller functionality. The final run at `b2d31d1` is recorded in
`corpus-final.txt` with the same 59 examples and 15-second budget.

| Requested rendering work | Result | Verified boundary |
| --- | --- | --- |
| Tessellation / 16 shader translations | NOT ACCEPTED; 14/16 | Patch method 0x320=0x201; control/evaluation stage contracts absent |
| MSAA and resolve, Example06/07 | NOT ACCEPTED | Multisample attachment/state gate |
| Viewport arrays, Example08 | NOT ACCEPTED | Inactive/swapped vertex attribute; viewport routing remains absent |
| Example09 raster after compute | NOT ACCEPTED | Polygon smoothing 0x1658=1; wide-line raster still absent |

`shader-audit.txt` records 521/521 decoded instructions and 14/16 translated
programs using symbolic typed textures. `rendering-blockers.txt` records fresh
scene failures. No unsupported stage, multisample operation, smoothing control,
or viewport selection was discarded to manufacture a passing scene.

Further boot gaps include broader service command coverage, actual network
sessions, general runtime linker behavior, patches/DLC selection and unsupported
NPDM capabilities. The application library's retail availability gate is still
present; direct runner CLI paths now share the tested packaged loader. The
synthetic package uses an own branch trampoline as rtld and does not prove
commercial relocation/import behavior. Its cryptographic integrity tables and
signatures are not a retail authenticity test.

Reproduce:

```sh
bash tools/switch/testtitles/run.sh
cargo test --offline --manifest-path tools/switch/core/Cargo.toml --features desktop --lib --test program_loader
SOJU_SWITCH_GPU=off cargo test --offline --manifest-path tools/switch/core/Cargo.toml --features desktop --lib
bash scripts/test-switch-gpu-accepted.sh
env -u SOJU_SWITCH_GPU -u SOJU_SWITCH_AUDIT_SHADERS -u SOJU_SWITCH_DOCKED SOJU_SWITCH_PROOF="$PWD/dist/boot-path-corpus-final" bash scripts/test-switch-examples.sh
cargo run --offline --manifest-path tools/switch/shader/Cargo.toml --example corpus_audit -- "$HOME/.local/share/soju/switch/examples" --symbolic-textures
```

The implementation used public service/layout documentation listed in
[core sources](../../../../tools/switch/core/SOURCES.md) and the installed
libnx public API declarations. Generated executables, containers and keys remain
outside tracked files. No push was performed.
