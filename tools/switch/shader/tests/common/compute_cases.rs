use super::*;
#[test]
fn shared_reduction_and_predicate_transfers_match_cpu() {
    let dir = gpu::setup();
    let mut ops = vec![
        0xf0c8000000000000 | PT | 10 | 0x21 << 20,
        0x3848000000000000 | PT | 11 | 10 << 8 | 2 << 20,
        0xef5c000000000000 | PT | 11 << 8,
    ];
    let barrier = 0xf0a8008000000000 | PT | 1 << 44 | 1 << 43 | 7 << 39;
    ops.push(barrier);
    for stride in [16, 8, 4, 2, 1] {
        ops.push(0xef4c000000000000 | PT | 11 << 8);
        ops.push(0xef4c000000000000 | PT | 1 | 11 << 8 | (stride * 4) << 20);
        ops.push(alu(0x5c10000000000000, 0, 0, 1));
        ops.push(0x3660000000000000 | PT | 7 | 10 << 8 | stride << 20 | 7 << 39 | 1 << 49);
        ops.push(0xef5c000000000000 | 11 << 8); // @P0 STS
        ops.push(barrier);
    }
    ops.push(0xef4c000000000000 | PT | 255 << 8);
    ops.push(0x38f0000000000000 | PT | 2 << 8 | 0x7f << 20);
    ops.push(0x38e8000000000000 | PT | 2 | 255 << 8 | 0x7f << 20);
    let input: Vec<_> = (0..32)
        .flat_map(|i| [i + 1, 0, i * 7, 0, 0, 0, 0, 0])
        .collect();
    let (binary, p) = program(&ops, 32);
    let shader = compute::translate(
        &binary[80..],
        &gpu::launch(32, 128),
        &gpu::options(input.len()),
    )
    .unwrap();
    let expected = cpu(&p, &input, 32, 32);
    for lane in expected.chunks_exact(8) {
        assert_eq!(lane[4], 528);
        assert_eq!(lane[6], lane[2] & 127);
    }
    let actual = gpu::execute(&dir, "reduction", &shader.source, &input, 32);
    assert_eq!(actual, expected);
    eprintln!("GPU equivalence: 32 reduction/predicate-transfer invocations, sum=528");
}

#[test]
fn memory_predicate_and_scheduling_classes_match_cpu() {
    let dir = gpu::setup();
    let input: Vec<_> = (0u32..32)
        .flat_map(|i| [i.wrapping_mul(0x654321u32), 31 - i, i * 2, 0, 0, 0, 0, 0])
        .collect();
    let cases = [
        (
            "generic_memory",
            vec![
                memory(0xa000000000000000 | 4 << 53 | 7 << 58, 0, 12),
                memory(0x8000000000000000 | 4 << 53 | 7 << 58, 2, 12),
            ],
        ),
        (
            "local_memory",
            vec![
                0xef54000000000000 | PT | 255 << 8,
                0xef44000000000000 | PT | 1 | 255 << 8,
            ],
        ),
        (
            "ldc",
            vec![0xef94000000000000 | PT | 1 | 255 << 8 | 1 << 36],
        ),
        (
            "predicates",
            vec![
                mov32(0, 1f32.to_bits()),
                mov32(1, 2f32.to_bits()),
                alu(0x5bb0000000000000, 7, 0, 1) | 7 << 39 | 1 << 48,
                0x5090000000000000 | PT | 15 | 7 << 29 | 7 << 39,
                0x38e8000000000000 | PT | 2 | 255 << 8 | 127 << 20,
            ],
        ),
        ("scheduling", vec![NOP, 0xf0f0000000070000]),
    ];
    for (name, ops) in cases {
        let (binary, p) = program(&ops, 32);
        let mut launch = gpu::launch(32, 0);
        launch.local_positive = 16;
        let shader =
            compute::translate(&binary[80..], &launch, &gpu::options(input.len())).unwrap();
        let expected = cpu(&p, &input, 0, 32);
        let actual = gpu::execute(&dir, name, &shader.source, &input, 32);
        assert_eq!(actual, expected, "{name}");
    }
    eprintln!("GPU equivalence: 160 memory/predicate/scheduling cases");
}

#[test]
fn reconvergence_and_break_loop_matches_cpu() {
    let dir = gpu::setup();
    let (binary, p) = program_with(
        |a| {
            a.branch(0xe290000000000000, "reconverge");
            a.branch(0xe2a0000000000000, "break");
            a.label("loop");
            a.emit(0x3810000000000000 | PT | 1 << 20);
            a.emit(alu(0x5b60000000000000, 7, 0, 1) | 7 << 39 | 1 << 49);
            a.emit(0xe34000000000000f | 1 << 19);
            a.branch(0xe24000000000000f | PT, "loop");
            a.label("break");
            a.emit(0xf0f800000000000f | PT);
            a.label("reconverge");
        },
        32,
    );
    let input: Vec<_> = (0u32..32).flat_map(|i| [0, i, 0, 0, 0, 0, 0, 0]).collect();
    let shader = compute::translate(
        &binary[80..],
        &gpu::launch(32, 0),
        &gpu::options(input.len()),
    )
    .unwrap();
    let expected = cpu(&p, &input, 0, 32);
    for (i, lane) in expected.chunks_exact(8).enumerate() {
        assert_eq!(lane[4], (i as u32).max(1));
    }
    assert_eq!(
        gpu::execute(&dir, "control_flow", &shader.source, &input, 32),
        expected
    );
    eprintln!("GPU equivalence: 32 reconvergence/break-loop cases");
}

#[test]
fn explicit_ftz_preserves_zero_signs_and_records_non_ftz_limit() {
    let dir = gpu::setup();
    let tiny = [1u32, 0x80000001, 0x007fffff, 0x807fffff];
    let input: Vec<_> = (0..32)
        .flat_map(|i| [tiny[i % 4], tiny[(i / 4) % 4], 0, 0, 0, 0, 0, 0])
        .collect();
    for (name, op) in [
        ("fadd_ftz", alu(0x5c58000000000000, 0, 0, 1) | 1 << 44),
        ("fmul_ftz", alu(0x5c68000000000000, 0, 0, 1) | 1 << 44),
        (
            "fmnmx_ftz",
            alu(0x5c60000000000000, 0, 0, 1) | 1 << 44 | 7 << 39,
        ),
        (
            "ffma_ftz",
            alu(0x5980000000000000, 0, 0, 1) | 2 << 39 | 1 << 53,
        ),
    ] {
        let (binary, p) = program(&[op], 32);
        let shader = compute::translate(
            &binary[80..],
            &gpu::launch(32, 0),
            &gpu::options(input.len()),
        )
        .unwrap();
        assert_eq!(
            gpu::execute(&dir, name, &shader.source, &input, 32),
            cpu(&p, &input, 0, 32),
            "{name}"
        );
    }
    let ops = [
        alu(0x5bb0000000000000, 7, 0, 1) | 7 << 39 | 1 << 48,
        0x38e8000000000000 | PT | 2 | 255 << 8 | 127 << 20,
    ];
    let (binary, p) = program(&ops, 32);
    let shader = compute::translate(
        &binary[80..],
        &gpu::launch(32, 0),
        &gpu::options(input.len()),
    )
    .unwrap();
    let actual = gpu::execute(&dir, "denormal_comparison", &shader.source, &input, 32);
    let ieee = cpu(&p, &input, 0, 32);
    let mut flushed = input.clone();
    for lane in flushed.chunks_exact_mut(8) {
        lane[0] &= 0x80000000;
        lane[1] &= 0x80000000;
    }
    let ftz = cpu(&p, &flushed, 0, 32);
    for i in 0..32 {
        assert!(actual[i * 8 + 6] == ieee[i * 8 + 6] || actual[i * 8 + 6] == ftz[i * 8 + 6]);
    }
    let differences = (0..32)
        .filter(|i| actual[i * 8 + 6] != ieee[i * 8 + 6])
        .count();
    eprintln!("GPU equivalence: 128 explicit FTZ cases; separate denormal probe: {differences}/32 differ from IEEE (documented allowed FTZ)");
}

#[test]
fn negative_local_address_bias_matches_gpu() {
    let dir = gpu::setup();
    let (binary, p) = program(
        &[
            0xef54000000000000 | PT | 255 << 8 | 0xfffffc << 20,
            0xef44000000000000 | PT | 1 | 255 << 8 | 0xfffffc << 20,
        ],
        32,
    );
    let input: Vec<_> = (0u32..32)
        .flat_map(|i| [i * 17 + 1, 0, 0, 0, 0, 0, 0, 0])
        .collect();
    let mut launch = gpu::launch(32, 0);
    launch.local_negative = 4;
    let shader = compute::translate(&binary[80..], &launch, &gpu::options(input.len())).unwrap();
    let mut m = Memory {
        global: input.clone(),
        local_negative: 4,
        ..Memory::default()
    };
    for id in 0..32 {
        let mut lane = Machine::default();
        lane.local = vec![0];
        lane.local_id = [id, 0, 0];
        lane.run(&p, &mut m).unwrap();
    }
    for lane in m.global.chunks_exact(8) {
        assert_eq!(lane[5], lane[0]);
    }
    assert_eq!(
        gpu::execute(&dir, "local_negative", &shader.source, &input, 32),
        m.global
    );
    eprintln!("GPU equivalence: 32 negative local address cases");
}
