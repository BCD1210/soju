# Virtual-memory probes

The original libnx guest passes 44 checks (`memory.txt`). It grows, shrinks and
regrows the heap with zero-fill checks; maps/unmaps physical pages; checks transfer
memory locking and close; creates a process-code alias, changes it to RX, executes
its own instruction sequence, unmaps, rewrites and executes the remapped sequence;
and touches distinct pages near 64 MiB, 2 GiB and the end of a 3 GiB heap.
Invalid handles, permissions, overlapping mappings, mismatched unmaps and shrinking
locked memory fail as expected. Generated executables remain under ignored dist.

The arena capacity is 3.5 GiB, with a 3 GiB heap ceiling. The homebrew configuration
now exports a real process handle. Process-code aliases require that handle and
are restricted to heap sources and the loader-code region. Physical unmap currently
requires an exact recorded allocation; partial splits and system-resource accounting
are not implemented. CreateCodeMemory/ControlCodeMemory and mapping a transfer
object into another process remain unsupported. This does not establish arbitrary
commercial memory-manager compatibility.

`memory-tests.txt`: 119 library, 9 kernel and 4 program-loader tests pass.
Reproduce with `bash tools/switch/testtitles/build.sh`, then the signed test_title
runner on `dist/testtitles/memory_probe.nro`.
