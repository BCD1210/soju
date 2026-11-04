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
    let mut a = Assembler::default();
    a.emit(0xf0c8000000000000 | PT | 240 | 0x21 << 20);
    a.emit(0xf0c8000000000000 | PT | 241 | 0x25 << 20);
    a.emit(alu(0x5c18000000000000, 240, 241, 240) | u64::from(width.trailing_zeros()) << 39);
    a.emit(0x3848000000000000 | PT | 240 | 240 << 8 | 5 << 20);
    for r in 0..4 {
        a.emit(memory(0xeed4000000000000, r, u64::from(r) * 4));
    }
    for &op in ops {
        a.emit(op);
    }
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
        ..Memory::default()
    };
    for group in 0..input.len() / 8 / width as usize {
        let mut lanes: Vec<_> = (0..width)
            .map(|i| {
                let mut lane = Machine::default();
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
