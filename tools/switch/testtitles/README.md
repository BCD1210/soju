# Self-built boot probes

Build and run on an Apple Silicon Mac with devkitPro/libnx and the Hypervisor entitlement:

```sh
bash tools/switch/testtitles/run.sh
```

`DEVKITPRO` defaults to `~/devkitpro/opt/devkitpro`. Source stays here; every ELF,
NRO, NSO, NACP, RomFS image, NSP, key file, signed runner, save, log and capture is
written under ignored `dist/testtitles`. No downloaded title or firmware is used.
The runner captures only these probes' debug strings. Production execution does
not print guest debug strings.

The six independent titles cover application startup, input, platform services,
offline services, storage and media. Application mode is selected explicitly:
the homebrew environment otherwise overrides the default to library-applet mode.
Every checked call produces PASS/FAIL; a stopped or incomplete probe is a failure.
The media probe runs docked, presents two resolutions and checks all 2,995,200
RGBA pixels against its source pattern. This checks buffer queues and display
conversion, not GPU shader rasterization.

`package_title` packages the storage NSO into a synthetic Program NCA with ExeFS
and RomFS plus a Control NCA. It generates fresh header, key-area and section keys
from the local random device, encrypts the headers and sections, and writes key
material only to a mode-0600 local file. The package is NSP-like: signature and
hash-tree authenticity are not claimed. Its own rtld is a branch trampoline into
libnx startup; it does not exercise a multi-module dynamic linker. No firmware is
needed. Two executions check committed-save persistence across processes.

The same `launch::load` function serves the probe harness, `--headless` and
`--play`. Packaged inputs use `SOJU_SWITCH_KEY_DIR`, or `keys` under the isolated
runner home, with `prod.keys` and optional `title.keys`. The test script unsets
that override and installs only its generated synthetic keys into its own home.
`SOJU_SWITCH_DOCKED=1` selects fixed docked operation/performance mode at boot;
mode transitions are not implemented.

These probes cover a bounded subset of each service. Friends list management,
TLS connections/handshakes, report submission, downloaded BCAT content, host
motion input and general commercial SDK startup are not established by them.
Unknown service commands remain explicit failures. See the committed
[results and remaining rendering gaps](../../../proof/rust-examples/regression/boot-path/README.md).
