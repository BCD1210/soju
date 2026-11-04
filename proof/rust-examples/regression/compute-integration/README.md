# Runner compute integration: scene acceptance still pending

B1C0 program-region and PCAS methods now reach the existing QMD/Metal compute
backend. Code discovery is bounded by the GPU allocation and an unconditional
EXIT. Graphics and compute share the bounded executable-span scanner. Constant
bindings come from QMD descriptors; the one supported writable global interval
comes from a nonempty deko driver storage descriptor. Multiple storage buffers
are rejected. Every dispatch completes and reads back before subsequent graphics
or CPU publication. Output and fences still pass through the IPC transaction.

QMD parsing now accepts the global-cache hint, the unlimited-call flag, and a
bounded call-stack reservation. These do not enable CALL instructions: unsupported
control flow still fails translation. Queue, dependent launch, semaphore,
resume and numerical modes remain rejected.

The runner-level synthetic test hand-encodes a store through a 64-bit address,
submits actual B1C0 methods, and checks staged output, untouched padding, no guest
write/fence publication before commit, and explicit rejection of a second storage
buffer. All 114 core library tests pass; the GPU suite passes with Metal validation.

Example09 now executes its compute dispatch and stops later at B197 0x1658=1.
Its source requests polygon smoothing and a 16-pixel line strip. This is NOT
scene acceptance, and no source-derived sinewave pixel check is claimed. The
other requested new graphics scenes remain unaccepted.

ABI references, inspected directly from the public homebrew graphics driver:

- [driver constbuf layout](https://github.com/devkitPro/deko3d/blob/373bfd43d8b2d805fa2eb7d406dbb823032df958/source/driver_constbuf.h)
- [binding counts](https://github.com/devkitPro/deko3d/blob/373bfd43d8b2d805fa2eb7d406dbb823032df958/include/deko3d.h)
- [compute binding and launch](https://github.com/devkitPro/deko3d/blob/373bfd43d8b2d805fa2eb7d406dbb823032df958/source/maxwell/gpu_compute.cpp)
- [QMD fields](https://github.com/devkitPro/deko3d/blob/373bfd43d8b2d805fa2eb7d406dbb823032df958/source/maxwell/compute_qmd.h)

No external implementation text is incorporated. Reproduce with the signed
runner and scripts/probe-switch-gpu.py --scene deko-example09. Logs and native
artifacts are in ignored dist/compute-probe3 and dist/default-metal-profile.
