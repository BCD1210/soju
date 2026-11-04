use soju_switch_gpu::{
    compute::{Engine, B1C0},
    memory::Memory,
};
fn qmd(version: u32) -> [u32; 64] {
    let mut q = [0; 64];
    q[8] = 0x100;
    q[12] = 32;
    q[13] = 1 | 1 << 16;
    q[17] = 128;
    q[18] = version | 32 << 16;
    q[19] = 1 | 1 << 16;
    q
}
fn bytes(q: [u32; 64]) -> Vec<u8> {
    q.into_iter().flat_map(u32::to_le_bytes).collect()
}
#[test]
fn launch_fields_and_unsupported_modes_are_checked() {
    let mut m = Memory::default();
    m.map(0x2000, bytes(qmd(0x17))).unwrap();
    let mut e = Engine::default();
    assert!(e
        .method(B1C0, 0x2bc, 2, &m)
        .unwrap_err()
        .0
        .contains("without QMD"));
    e.method(B1C0, 0x1608, 0x12, &m).unwrap();
    e.method(B1C0, 0x160c, 0x34560000, &m).unwrap();
    e.method(B1C0, 0x2b4, 0x20, &m).unwrap();
    for version in [0x06, 0x17] {
        let mut q = qmd(version);
        q[6] = (1 << 6) | (1 << 12);
        q[11] = 1 << 26;
        q[47] = 0x800;
        q[20] = 1 << 2;
        q[33] = 0x56780000;
        q[34] = 0x34 | 16 << 15;
        m.write(0x2000, &bytes(q)).unwrap();
        let d = e.method(B1C0, 0x2bc, 3, &m).unwrap().unwrap();
        assert_eq!(d.program(), 0x1234560100);
        assert_eq!(d.groups(), [32, 1, 1]);
        assert_eq!(d.launch().local_size, [32, 1, 1]);
        assert_eq!(d.launch().shared_bytes, 128);
        assert_eq!(d.constant(2).unwrap().address, 0x3456780000);
        assert_eq!(d.constant(2).unwrap().size, 16);
    }
    for (word, bits) in [
        (6, 1 << 8),
        (6, 1 << 10),
        (6, 1 << 13),
        (14, 1),
        (11, 1 << 24),
        (20, 1 << 8),
        (47, 1 << 24),
    ] {
        let mut q = qmd(0x17);
        q[word] |= bits;
        m.write(0x2000, &bytes(q)).unwrap();
        assert!(e
            .method(B1C0, 0x2bc, 2, &m)
            .unwrap_err()
            .0
            .contains("unsupported QMD"));
    }
    for (word, value) in [(12, 0), (18, 0x18 | 32 << 16), (19, 0), (17, 49 * 1024)] {
        let mut q = qmd(0x17);
        q[word] = value;
        m.write(0x2000, &bytes(q)).unwrap();
        assert!(e.method(B1C0, 0x2bc, 2, &m).is_err());
    }
    assert!(e.method(B1C0, 0x2b8, 0, &m).is_err());
    assert!(e.method(B1C0, 0x2bc, 4, &m).is_err());
    assert!(e.method(B1C0, 0x1608, 256, &m).is_err());
}

#[cfg(target_os = "macos")]
#[test]
fn qmd_shared_kernel_writes_image_and_preserves_order() {
    use soju_switch_gpu::backend::Backend;
    use soju_switch_shader::msl::{GlobalMemory, Options};
    const PT: u64 = 7 << 16;
    let ops = [
        0xf0c8000000000000 | PT | 10 | 0x21 << 20, // thread x
        0xf0c8000000000000 | PT | 11 | 0x25 << 20, // CTA x
        0x3848000000000000 | PT | 11 | 11 << 8 | 5 << 20, // group*32
        0x5c10000000000000 | PT | 12 | 11 << 8 | 10 << 20, // linear index
        0x3848000000000000 | PT | 12 | 12 << 8 | 2 << 20, // byte index
        0x1c00000000000000 | PT | 12 | 12 << 8 | 0x10000 << 20, // global address
        0x4c98000000000000 | PT | 15 << 39 | 2 << 34, // cb2[0] colour
        0x5c10000000000000 | PT | 10 << 20,        // red += thread x
        0x3848000000000000 | PT | 13 | 10 << 8 | 2 << 20, // shared byte index
        0xef5c000000000000 | PT | 13 << 8,         // shared[index] = r0
        0xf0a8008000000000 | PT | 1 << 44 | 1 << 43 | 7 << 39, // barrier
        0x3840000000000000 | PT | 13 | 13 << 8 | 124 << 20 | 7 << 48 | 2 << 41, // reverse shared lane
        0xef4c000000000000 | PT | 1 | 13 << 8, // r1 = shared[31-index]
        0xeedc000000000000 | PT | 1 | 12 << 8 | 4 << 48, // global[address] = r1
        0xe30000000000000f | PT,
    ];
    let mut code = Vec::new();
    for chunk in ops.chunks(3) {
        code.extend(0x001f8000fc0007e0u64.to_le_bytes());
        for word in chunk {
            code.extend(word.to_le_bytes());
        }
    }
    let mut memory = Memory::default();
    memory.map(0x1000, code.clone()).unwrap();
    memory.map(0x2000, bytes(qmd(0x17))).unwrap();
    memory.map(0x3000, vec![0; 16]).unwrap();
    memory.map(0x10000, vec![0; 4096]).unwrap();
    // A second independent output window deliberately starts with identical bytes.
    memory.map(0x20000, vec![0; 4096]).unwrap();
    let mut backend = Backend::default();
    backend
        .compute_program(
            0x1000,
            code.len(),
            Options {
                global_memory: Some(GlobalMemory {
                    buffer: 0,
                    base_address: 0x10000,
                    bytes: 4096,
                }),
                ..Options::default()
            },
        )
        .unwrap();
    let mut engine = Engine::default();
    engine.method(B1C0, 0x160c, 0xf00, &memory).unwrap();
    engine.method(B1C0, 0x2b4, 0x20, &memory).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/compute-checks");
    for version in [0x06, 0x17] {
        let mut q = qmd(version);
        q[6] = (1 << 6) | (1 << 12);
        q[11] = 1 << 26;
        q[47] = 0x800;
        q[20] = 1 << 2;
        q[33] = 0x3000;
        q[34] = 16 << 15;
        memory.write(0x2000, &bytes(q)).unwrap();
        memory.write(0x3000, &0xff804000u32.to_le_bytes()).unwrap();
        let dispatch = engine.method(B1C0, 0x2bc, 2, &memory).unwrap().unwrap();
        backend.prepare_compute(&dispatch, &memory).unwrap();
        let diagnostic = backend.submit(&root).unwrap().wait().unwrap();
        assert!(diagnostic.contains("dispatches=1"));
        assert!(
            memory
                .read(0x10000, 4096)
                .unwrap()
                .chunks_exact(4)
                .all(|p| p == [0; 4])
                || version == 0x17
        );
        let report = backend.readback(&root, &mut memory).unwrap();
        assert!(report.images.is_empty());
        for (i, p) in memory
            .read(0x10000, 4096)
            .unwrap()
            .chunks_exact(4)
            .enumerate()
        {
            assert_eq!(p, [(31 - i % 32) as u8, 64, 128, 255]);
        }
        std::fs::write(
            root.join(format!("qmd-{version:x}.rgba")),
            memory.read(0x10000, 4096).unwrap(),
        )
        .unwrap();
    }
    // Two writable resources with identical initial bytes must not share a native buffer.
    let mut other_ops = ops.to_vec();
    other_ops[5] = 0x1c00000000000000 | PT | 12 | 12 << 8 | 0x20000 << 20;
    other_ops.insert(13, 0xeed0000000000000 | PT | 2 | 12 << 8 | 4 << 48);
    other_ops.insert(14, 0x5c10000000000000 | PT | 1 | 1 << 8 | 2 << 20);
    let mut other_code = Vec::new();
    for chunk in other_ops.chunks(3) {
        other_code.extend(0x001f8000fc0007e0u64.to_le_bytes());
        for word in chunk {
            other_code.extend(word.to_le_bytes());
        }
    }
    memory.map(0x4000, other_code.clone()).unwrap();
    backend
        .compute_program(
            0x4000,
            other_code.len(),
            Options {
                global_memory: Some(GlobalMemory {
                    buffer: 0,
                    base_address: 0x20000,
                    bytes: 4096,
                }),
                ..Options::default()
            },
        )
        .unwrap();
    memory.write(0x10000, &[0; 4096]).unwrap();
    let dispatch = engine.method(B1C0, 0x2bc, 2, &memory).unwrap().unwrap();
    backend.prepare_compute(&dispatch, &memory).unwrap();
    let mut q = qmd(0x17);
    q[8] = 0x3100;
    q[20] = 1 << 2;
    q[33] = 0x3000;
    q[34] = 16 << 15;
    memory.write(0x2000, &bytes(q)).unwrap();
    memory.write(0x3000, &0xff208000u32.to_le_bytes()).unwrap();
    let second = engine.method(B1C0, 0x2bc, 2, &memory).unwrap().unwrap();
    backend.prepare_compute(&second, &memory).unwrap();
    // Another launch on the same output observes its preceding GPU writes.
    backend.prepare_compute(&second, &memory).unwrap();
    assert!(backend
        .submit(&root)
        .unwrap()
        .wait()
        .unwrap()
        .contains("dispatches=3"));
    backend.readback(&root, &mut memory).unwrap();
    for (address, green, blue, alpha, factor) in
        [(0x10000, 64, 128, 255, 1), (0x20000, 0, 65, 254, 2)]
    {
        for (i, p) in memory
            .read(address, 4096)
            .unwrap()
            .chunks_exact(4)
            .enumerate()
        {
            assert_eq!(p, [factor * (31 - i % 32) as u8, green, blue, alpha]);
        }
    }
    // Restore the first descriptor before checking an unsynchronized CPU write.
    let mut q = qmd(0x17);
    q[20] = 1 << 2;
    q[33] = 0x3000;
    q[34] = 16 << 15;
    memory.write(0x2000, &bytes(q)).unwrap();
    let dispatch = engine.method(B1C0, 0x2bc, 2, &memory).unwrap().unwrap();
    backend.prepare_compute(&dispatch, &memory).unwrap();
    backend.submit(&root).unwrap().wait().unwrap();
    memory.write(0x10000, &[1, 2, 3, 4]).unwrap();
    assert!(backend
        .readback(&root, &mut memory)
        .unwrap_err()
        .0
        .contains("overlaps pending compute"));
    assert!(backend
        .prepare_compute(&dispatch, &memory)
        .unwrap_err()
        .0
        .contains("overlaps pending compute"));
    eprintln!("B1C0 QMD 0.6/1.7 shared-memory image checks: 4096 pixels");
}
