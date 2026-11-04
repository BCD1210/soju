#![cfg(target_os = "macos")]
mod common;
#[path = "common/gpu.rs"]
mod gpu;
use common::*;
use soju_switch_shader::{
    compute, decoder,
    ir::Program,
    reference::{run_group, Machine, Memory},
};
fn memory(op: u64, r: u8, offset: u64) -> u64 {
    op | PT | u64::from(r) | 240 << 8 | offset << 20
}
fn program(ops: &[u64], width: u32) -> (Vec<u8>, Program) {
    program_with(
        |a| {
            for &op in ops {
                a.emit(op);
            }
        },
        width,
    )
}
fn program_with(body: impl FnOnce(&mut Assembler), width: u32) -> (Vec<u8>, Program) {
    let mut a = Assembler::default();
    a.emit(0xf0c8000000000000 | PT | 240 | 0x21 << 20);
    a.emit(0xf0c8000000000000 | PT | 241 | 0x25 << 20);
    a.emit(alu(0x5c18000000000000, 240, 241, 240) | u64::from(width.trailing_zeros()) << 39);
    a.emit(0x3848000000000000 | PT | 240 | 240 << 8 | 5 << 20);
    for r in 0..4 {
        a.emit(memory(0xeed4000000000000, r, u64::from(r) * 4));
    }
    body(&mut a);
    for r in 0..4 {
        a.emit(memory(0xeedc000000000000, r, 16 + u64::from(r) * 4));
    }
    a.emit(EXIT);
    let b = a.finish(fragment_header());
    let p = Program::build(&decoder::decode(&b[80..]).unwrap()).unwrap();
    (b, p)
}
fn cpu(p: &Program, input: &[u32], shared: usize, width: u32) -> Vec<u32> {
    let mut m = Memory {
        global: input.to_vec(),
        shared: vec![0; shared],
        constants: std::collections::BTreeMap::from([(1, input.to_vec())]),
        ..Memory::default()
    };
    for group in 0..input.len() / 8 / width as usize {
        let mut lanes: Vec<_> = (0..width)
            .map(|i| {
                let mut lane = Machine::default();
                lane.local = vec![0; 4];
                lane.local_id = [i, 0, 0];
                lane.group_id = [group as u32, 0, 0];
                lane
            })
            .collect();
        run_group(p, &mut lanes, &mut m).unwrap();
    }
    m.global
}
#[test]
fn arithmetic_grid_matches_cpu_reference() {
    let dir = gpu::setup();
    let cases: [(&str, u64, bool); 30] = [
        ("iadd", alu(0x5c10000000000000, 0, 0, 1), false),
        ("iadd3", alu(0x5cc0000000000000, 0, 0, 1) | 2 << 39, false),
        ("iscadd", alu(0x5c18000000000000, 0, 0, 1) | 3 << 39, false),
        ("imad", alu(0x5a00000000000000, 0, 0, 1) | 2 << 39, false),
        ("xmad", alu(0x5b00000000000000, 0, 0, 1) | 2 << 39, false),
        ("shl", alu(0x5c48000000000000, 0, 0, 1), false),
        ("shr", alu(0x5c28000000000000, 0, 0, 1), false),
        (
            "shr_signed",
            alu(0x5c28000000000000, 0, 0, 1) | 1 << 48,
            false,
        ),
        ("bfe", alu(0x5c00000000000000, 0, 0, 1), false),
        ("bfi", alu(0x5bf0000000000000, 0, 0, 1) | 2 << 39, false),
        ("popc", alu(0x5c08000000000000, 0, 0, 1), false),
        ("flo", alu(0x5c30000000000000, 0, 0, 1), false),
        (
            "lop",
            alu(0x5c40000000000000, 0, 0, 1) | 7 << 48 | 2 << 41,
            false,
        ),
        (
            "lop3",
            alu(0x5be0000000000000, 0, 0, 1) | 7 << 48 | 2 << 39 | 0x96 << 28,
            false,
        ),
        ("sel", alu(0x5ca0000000000000, 0, 0, 1) | 7 << 39, false),
        ("imnmx", alu(0x5c20000000000000, 0, 0, 1) | 7 << 39, false),
        ("i2i", alu(0x5ce0000000000000, 0, 0, 1) | 0xa00, false),
        ("fadd", alu(0x5c58000000000000, 0, 0, 1), true),
        ("fmul", alu(0x5c68000000000000, 0, 0, 1), true),
        ("ffma", alu(0x5980000000000000, 0, 0, 1) | 2 << 39, true),
        ("fmnmx", alu(0x5c60000000000000, 0, 0, 1) | 7 << 39, true),
        (
            "f2f",
            alu(0x5ca8000000000000, 0, 0, 1) | 0xa00 | 8 << 39,
            true,
        ),
        (
            "f2i",
            alu(0x5cb0000000000000, 0, 0, 1) | 0xa00 | 3 << 39,
            true,
        ),
        ("i2f", alu(0x5cb8000000000000, 0, 0, 1) | 0xa00, false),
        ("mufu_sqrt", alu(0x5080000000000000, 0, 0, 8), true),
        (
            "iset",
            alu(0x5b50000000000000, 0, 0, 1) | 7 << 39 | 1 << 49,
            false,
        ),
        (
            "fset",
            alu(0x5800000000000000, 0, 0, 1) | 7 << 39 | 1 << 48,
            true,
        ),
        ("mov", alu(0x5c98000000000000, 0, 0, 1) | 15 << 39, false),
        (
            "shl_wrap",
            alu(0x5c48000000000000, 0, 0, 1) | 1 << 39,
            false,
        ),
        (
            "bfe_signed",
            alu(0x5c00000000000000, 0, 0, 1) | 1 << 48,
            false,
        ),
    ];
    let ints = [
        0,
        1,
        31,
        32,
        33,
        255,
        0xffff,
        0x80000000,
        u32::MAX,
        0x12345678,
        0x100,
        0x1f08,
        0x2000,
        0xff00,
        0x0101,
        0x101f,
    ];
    let floats = [
        0.0,
        0.25,
        0.5,
        1.0,
        2.0,
        3.5,
        16.0,
        1024.0,
        -0.0,
        -0.25,
        -1.0,
        -3.5,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
        16777216.0,
    ];
    let mut count = 0;
    for (name, op, float) in cases {
        let input: Vec<_> = (0..256)
            .flat_map(|i| {
                let v = |j: usize| if float { floats[j].to_bits() } else { ints[j] };
                [v(i % 16), v(i / 16), v((i + 3) % 16), 0, 0, 0, 0, 0]
            })
            .collect();
        let (binary, p) = program(&[op], 32);
        let shader = compute::translate(
            &binary[80..],
            &gpu::launch(32, 0),
            &gpu::options(input.len()),
        )
        .unwrap();
        let expected = cpu(&p, &input, 0, 32);
        let actual = gpu::execute(&dir, name, &shader.source, &input, 32);
        for (i, (a, b)) in actual.iter().zip(&expected).enumerate() {
            let both_nan = float && f32::from_bits(*a).is_nan() && f32::from_bits(*b).is_nan();
            assert!(
                a == b || both_nan,
                "{name} word {i}: GPU {a:08x} CPU {b:08x}"
            );
        }
        count += 256;
    }
    eprintln!("GPU equivalence: {count} arithmetic program/input cases");
}

#[path = "common/compute_cases.rs"]
mod compute_cases;

#[test]
fn reduced_functions_match_native_math() {
    let dir = gpu::setup();
    for (name, mode, function) in [("rro_sin", 0, 1), ("rro_cos", 0, 0), ("rro_exp2", 1, 2)] {
        let input: Vec<u32> = (0..32)
            .flat_map(|i| {
                let v = (i as f32 - 16.) * 0.375;
                [v.to_bits(), 0, 0, 0, 0, 0, 0, 0]
            })
            .collect();
        let (binary, p) = program(
            &[
                0x5c90000000000000 | PT | mode << 39,
                0x5080000000000000 | PT | function << 20,
            ],
            32,
        );
        let shader = compute::translate(
            &binary[80..],
            &gpu::launch(32, 0),
            &gpu::options(input.len()),
        )
        .unwrap();
        let actual = gpu::execute(&dir, name, &shader.source, &input, 32);
        let expected = cpu(&p, &input, 0, 32);
        for i in 0..32 {
            let a = f32::from_bits(actual[i * 8 + 4]);
            let b = f32::from_bits(expected[i * 8 + 4]);
            assert!(
                (a - b).abs() <= 2e-6 * b.abs().max(1.),
                "{name} lane {i}: {a} != {b}"
            );
        }
    }
}

#[test]
fn reconverging_diamond_keeps_mutated_condition_in_its_arm() {
    let dir = gpu::setup();
    let (binary, p) = program_with(
        |a| {
            a.emit(alu(0x5b60000000000000, 7, 0, 1) | 7 << 39 | 1 << 49);
            a.branch(0xe290000000000000, "join");
            a.branch(0xe24000000000000f, "taken");
            a.emit(mov32(2, 123));
            a.emit(alu(0x5b60000000000000, 7, 255, 1) | 7 << 39 | 1 << 49);
            a.emit(0xf0f800000000000f | PT);
            a.label("taken");
            a.emit(mov32(2, 456));
            a.emit(0xf0f800000000000f | PT);
            a.label("join");
        },
        32,
    );
    let input: Vec<_> = (0..32).flat_map(|i| [i, 16, 0, 0, 0, 0, 0, 0]).collect();
    let shader = compute::translate(
        &binary[80..],
        &gpu::launch(32, 0),
        &gpu::options(input.len()),
    )
    .unwrap();
    let expected = cpu(&p, &input, 0, 32);
    for (i, lane) in expected.chunks_exact(8).enumerate() {
        assert_eq!(lane[6], if i < 16 { 456 } else { 123 });
    }
    assert_eq!(
        gpu::execute(&dir, "diamond", &shader.source, &input, 32),
        expected
    );
}

#[test]
fn flush_to_zero_comparisons_handle_signed_subnormals() {
    let dir = gpu::setup();
    let input: Vec<_> = (0..32)
        .flat_map(|i| [if i % 2 == 0 { 1 } else { 0x80000001 }, 0, 0, 0, 0, 0, 0, 0])
        .collect();
    let (binary, p) = program(
        &[
            alu(0x5bb0000000000000, 7, 0, 1) | 7 << 39 | 2 << 48 | 1 << 47,
            0x38e8000000000000 | PT | 2 | 255 << 8 | 127 << 20,
        ],
        32,
    );
    let shader = compute::translate(
        &binary[80..],
        &gpu::launch(32, 0),
        &gpu::options(input.len()),
    )
    .unwrap();
    let expected = cpu(&p, &input, 0, 32);
    for lane in expected.chunks_exact(8) {
        assert_eq!(lane[6] & 1, 1);
    }
    assert_eq!(
        gpu::execute(&dir, "ftz_compare", &shader.source, &input, 32),
        expected
    );
}

#[test]
fn zero_wins_multiply_covers_exceptional_operands() {
    let dir = gpu::setup();
    let values = [
        0.,
        -0.,
        f32::from_bits(1),
        f32::from_bits(0x80000001),
        f32::INFINITY,
        f32::NAN,
        2.,
        -3.,
    ];
    let input: Vec<_> = (0..64)
        .flat_map(|i| {
            [
                values[i % 8].to_bits(),
                values[i / 8].to_bits(),
                0,
                0,
                0,
                0,
                0,
                0,
            ]
        })
        .collect();
    let (binary, p) = program(&[alu(0x5c68000000000000, 0, 0, 1) | 2 << 44], 32);
    let shader = compute::translate(
        &binary[80..],
        &gpu::launch(32, 0),
        &gpu::options(input.len()),
    )
    .unwrap();
    let actual = gpu::execute(&dir, "fmz", &shader.source, &input, 32);
    let expected = cpu(&p, &input, 0, 32);
    for i in 0..64 {
        let (a, b) = (actual[i * 8 + 4], expected[i * 8 + 4]);
        assert!(
            a == b || f32::from_bits(a).is_nan() && f32::from_bits(b).is_nan(),
            "lane {i}: {a:x} {b:x}"
        );
        if i % 8 < 4 || i / 8 < 4 {
            assert_eq!(a, 0);
        }
    }
}

#[test]
fn carry_addition_propagates_into_high_word() {
    let dir = gpu::setup();
    let input: Vec<_> = (0u32..32)
        .flat_map(|i| [u32::MAX - i, i + 1, 17, 23, 0, 0, 0, 0])
        .collect();
    let (binary, p) = program(
        &[
            alu(0x5c10000000000000, 0, 0, 1) | 1 << 47,
            alu(0x5c10000000000000, 2, 2, 3) | 1 << 43,
        ],
        32,
    );
    let shader = compute::translate(
        &binary[80..],
        &gpu::launch(32, 0),
        &gpu::options(input.len()),
    )
    .unwrap();
    let expected = cpu(&p, &input, 0, 32);
    for lane in expected.chunks_exact(8) {
        assert_eq!((lane[4], lane[6]), (0, 41));
    }
    assert_eq!(
        gpu::execute(&dir, "carry-add", &shader.source, &input, 32),
        expected
    );
}

#[test]
fn flushed_conversion_rounds_subnormals_after_flushing() {
    let dir = gpu::setup();
    let input: Vec<_> = (0..32)
        .flat_map(|i| [if i % 2 == 0 { 1 } else { 0x80000001 }, 0, 0, 0, 0, 0, 0, 0])
        .collect();
    for (name, mode) in [("ftz_floor", 1), ("ftz_ceil", 2)] {
        let (binary, p) = program(
            &[0x5cb0000000000000 | PT | 0xa00 | 1 << 12 | 1 << 44 | mode << 39],
            32,
        );
        let shader = compute::translate(
            &binary[80..],
            &gpu::launch(32, 0),
            &gpu::options(input.len()),
        )
        .unwrap();
        let expected = cpu(&p, &input, 0, 32);
        for lane in expected.chunks_exact(8) {
            assert_eq!(lane[4], 0);
        }
        assert_eq!(
            gpu::execute(&dir, name, &shader.source, &input, 32),
            expected
        );
    }
}

#[test]
fn unsigned_short_conversion_clamps_before_export() {
    let dir = gpu::setup();
    let values = [
        -1f32,
        0.,
        0.6,
        255.,
        65535.,
        65536.,
        f32::INFINITY,
        f32::NAN,
    ];
    let input: Vec<_> = (0..32)
        .flat_map(|i| [values[i % 8].to_bits(), 0, 0, 0, 0, 0, 0, 0])
        .collect();
    let (binary, p) = program(&[0x5cb0000000000000 | PT | 0x900 | 1 << 44], 32);
    let shader = compute::translate(
        &binary[80..],
        &gpu::launch(32, 0),
        &gpu::options(input.len()),
    )
    .unwrap();
    let expected = cpu(&p, &input, 0, 32);
    for (i, lane) in expected.chunks_exact(8).enumerate() {
        assert_eq!(lane[4], [0, 0, 1, 255, 65535, 65535, 65535, 0][i % 8]);
    }
    assert_eq!(
        gpu::execute(&dir, "f2i-u16", &shader.source, &input, 32),
        expected
    );
}
