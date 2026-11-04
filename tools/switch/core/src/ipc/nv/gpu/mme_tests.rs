use super::mme::MacroUpload;
use super::tests::{Available, commands};
use super::*;
const PROGRAM: [u32; 17] = [
    0x00dac215, 0x00db0315, 0x0012d710, 0x020c0415, 0x00004211, 0x00180611, 0x0014bf10, 0x0000f807,
    0x02100021, 0x00800611, 0x00130f10, 0x0014bf10, 0x0000f807, 0x00000841, 0x00000611, 0x002ec0a1,
    0x00003041,
];
fn upload(mme: &mut MacroUpload, entry: u32, start: u32, words: &[u32]) {
    mme.write(0x11c, entry).unwrap();
    mme.write(0x120, start).unwrap();
    mme.write(0x114, start).unwrap();
    for &word in words {
        mme.write(0x118, word).unwrap();
    }
}
#[test]
fn macro_branch_delays_and_sequential_state_are_transactional() {
    let mut state = ExecutionState::default()
        .stage(&commands(0, &[(0, 0xb197)]), &Available)
        .unwrap();
    upload(&mut state.mme, 5, 45, &PROGRAM);
    let mut second = PROGRAM;
    second[3] = 0x02100415;
    second[8] = 0x020c0021;
    upload(&mut state.mme, 6, 28, &second);
    let before = state.clone();
    let commands = commands(0, &[(0x3828, 0x40), (0x3830, 0x30)]);
    let staged = state.stage(&commands, &Available).unwrap();
    assert_eq!(staged.registers[&(0, 0x2100)], 0x40);
    assert_eq!(staged.registers[&(0, 0x20c0)], 0x30);
    assert_eq!(staged.registers[&(0, 0x2ec)], 0x60);
    assert_eq!(state, before);
    let mut invalid = commands;
    invalid.push(Method {
        address: 0x1000,
        subchannel: 0,
        method: 0x2c4,
        value: 0,
    });
    assert!(state.stage(&invalid, &Available).is_err());
    assert_eq!(state, before);
}
#[test]
fn vertex_parameters_and_blend_merge_match_known_program_outputs() {
    let mut state = ExecutionState::default()
        .stage(&commands(0, &[(0, 0xb197)]), &Available)
        .unwrap();
    upload(
        &mut state.mme,
        1,
        16,
        &[
            0x201, 0x301, 0x401, 0x501, 0x11004612, 0x09004712, 0x05c07621, 0x2041, 0x2841,
            0x05f03f21, 0x10c1, 0x1841,
        ],
    );
    let vertex = commands(
        0,
        &[
            (0x3808, 0),
            (0x380c, 2),
            (0x380c, 0x0583001f),
            (0x380c, 2),
            (0x380c, 0x05830000),
        ],
    );
    let staged = state.stage(&vertex, &Available).unwrap();
    assert_eq!(staged.registers[&(0, 0x1c04)], 2);
    assert_eq!(staged.registers[&(0, 0x1c08)], 0x05830000);
    assert_eq!(staged.registers[&(0, 0x1f04)], 0x0583001f);
    assert!(state.stage(&vertex[..4], &Available).is_err());
    upload(
        &mut state.mme,
        2,
        7,
        &[
            0x05360021, 0x00404042, 0x00424042, 0x00444042, 0x00464042, 0x00484042, 0x004a4042,
            0x004c40c2, 0x004e4042,
        ],
    );
    for value in [0xff, 0x81, 0] {
        let staged = state
            .stage(&commands(0, &[(0x3810, value)]), &Available)
            .unwrap();
        for i in 0..8 {
            assert_eq!(staged.registers[&(0, 0x1360 + 4 * i)], (value >> i) & 1);
        }
    }
}
#[test]
fn mme_upload_limits_and_invalid_execution_do_not_wrap() {
    let mut mme = MacroUpload::default();
    mme.write(0x114, 16383).unwrap();
    mme.write(0x118, 7).unwrap();
    let before = mme.clone();
    assert!(mme.write(0x118, 8).is_err());
    assert_eq!(mme, before);
    mme.write(0x11c, 127).unwrap();
    mme.write(0x120, 0).unwrap();
    let before = mme.clone();
    assert!(mme.write(0x120, 0).is_err());
    assert_eq!(mme, before);
    for words in [
        vec![0x8807, 0x8027, 0x11, 0x91, 0x11],
        vec![0x400021, 0x841, 0x400215, 0x404021, 0x1041, 0x91, 0x11],
        vec![0x91],
        vec![7, 0x11],
        vec![0xf],
        vec![0x30],
    ] {
        let mut mme = MacroUpload::default();
        upload(&mut mme, 0, 0, &words);
        assert!(mme.run(0, 1, &[], |_| Ok(0)).is_err());
    }
    let mut mme = MacroUpload::default();
    upload(&mut mme, 0, 0, &[0x4011, 0x100021, 0x41, 0x91, 0x11]);
    assert_eq!(mme.run(0, 0, &[], |_| Ok(0)).unwrap(), vec![(0x100, 0)]);
}

#[test]
fn macro_parameter_destinations_preserve_send_and_fetch_order() {
    let selector = ((0x1040u32) << 14) | 0x21;
    type Case<'a> = (&'a [u32], u32, &'a [u32], &'a [(u32, u32)]);
    let cases: &[Case<'_>] = &[
        (
            &[selector, 0xa31, 0x1041, 0x91, 0x11],
            7,
            &[9],
            &[(0x100, 7), (0x104, 9)],
        ),
        (&[0xa51, 0x1041, 0x91, 0x11], 0x1040, &[7], &[(0x100, 7)]),
        (
            &[0xa61, 0x1041, 0x91, 0x11],
            0x1040,
            &[9],
            &[(0x100, 9), (0x104, 0x1040)],
        ),
        (
            &[0xa71, 0x1041, 0x91, 0x11],
            0x3040,
            &[],
            &[(0x100, 3), (0x10c, 0x3040)],
        ),
    ];
    for &(program, argument, parameters, expected) in cases {
        let mut mme = MacroUpload::default();
        upload(&mut mme, 0, 0, program);
        assert_eq!(
            mme.run(0, argument, parameters, |_| Ok(0)).unwrap(),
            expected
        );
        if !parameters.is_empty() {
            assert!(mme.run(0, argument, &[], |_| Ok(0)).is_err());
        }
    }
}

#[test]
fn taken_branch_suppresses_exit_in_delay_slot() {
    let mut mme = MacroUpload::default();
    upload(
        &mut mme,
        0,
        0,
        &[
            (0x40 << 14) | 0x21,
            (3 << 14) | 7,
            0x91,
            0xf,
            0x841,
            0x91,
            0x11,
        ],
    );
    assert_eq!(mme.run(0, 17, &[], |_| Ok(0)).unwrap(), [(0x100, 17)]);
}

#[test]
fn branch_exit_modifier_only_exits_when_branch_is_not_taken() {
    let mut mme = MacroUpload::default();
    upload(
        &mut mme,
        0,
        0,
        &[
            (0x1040 << 14) | 0x21,
            (3 << 14) | (1 << 11) | 0x97,
            0x841,
            0xf,
            0x91,
            0x841,
        ],
    );
    assert_eq!(mme.run(0, 0, &[], |_| Ok(0)).unwrap(), [(0x100, 0)]);
    assert_eq!(
        mme.run(0, 3, &[], |_| Ok(0)).unwrap(),
        [(0x100, 3), (0x104, 3)]
    );
}

#[test]
fn scratch_read_observes_latest_emission_but_effect_reads_stay_rejected() {
    for address in [0x3400u32, 0x100] {
        let mut mme = MacroUpload::default();
        upload(
            &mut mme,
            0,
            0,
            &[
                ((address / 4) << 14) | 0x21,
                (7 << 14) | 0x41,
                (9 << 14) | 0x41,
                ((address / 4) << 14) | 0x295,
                (2 << 11) | 0x41,
            ],
        );
        let result = mme.run(0, 0, &[], |_| Ok(123));
        if address == 0x3400 {
            assert_eq!(result.unwrap(), [(address, 7), (address, 9), (address, 9)]);
        } else {
            assert!(result.is_err());
        }
    }
}

#[test]
fn macro_register_arithmetic_and_boolean_ops_emit_exact_values() {
    for (alu, expected) in [
        (0, 2),
        (2, 0xffff_fffc),
        (8, 0xffff_fffc),
        (11, 0xffff_fffc),
        (12, 0xffff_fffc),
    ] {
        let mut mme = MacroUpload::default();
        upload(
            &mut mme,
            0,
            0,
            &[
                ((0x3400 / 4) << 14) | 0x21,
                0x201,
                (alu << 17) | (2 << 14) | (1 << 11) | 0xc0,
                0x11,
            ],
        );
        assert_eq!(
            mme.run(0, u32::MAX, &[3], |_| Ok(0)).unwrap(),
            [(0x3400, expected)]
        );
    }
}

#[test]
fn variable_bitfield_extract_shifts_use_register_positions() {
    for op in [3, 4] {
        let mut mme = MacroUpload::default();
        // r1 is the shift argument; r2 consumes the data parameter.
        let word = op
            | 1 << 4
            | 3 << 8
            | 1 << 11
            | 2 << 14
            | 4 << 22
            | if op == 3 { 8 << 27 } else { 8 << 17 };
        upload(
            &mut mme,
            0,
            0,
            &[0x201, word, 0x21 | 0x100 << 14, 0xc1 | 3 << 11, 0x11],
        );
        assert_eq!(
            mme.run(0, 4, &[0xabcde], |_| Ok(0)).unwrap(),
            vec![(0x400, if op == 3 { 0xd00 } else { 0xc0 })]
        );
        assert!(mme.run(0, 32, &[0xabcde], |_| Ok(0)).is_err());
    }
}

#[test]
fn counted_fill_writes_every_requested_register() {
    let mut mme = MacroUpload::default();
    upload(
        &mut mme,
        0,
        0,
        &[
            (1 << 11) | (2 << 8) | 0x51,
            (3 << 8) | 1,
            (0x3ffff << 14) | (2 << 11) | (2 << 8) | 0x11,
            (0x3ffff << 14) | (2 << 11) | 0x17,
            (3 << 11) | 0xc1,
            0x11,
        ],
    );
    assert_eq!(
        mme.run(0, 0x8904, &[5, 1], |_| Ok(0)).unwrap(),
        [
            (0x2410, 1),
            (0x2430, 1),
            (0x2450, 1),
            (0x2470, 1),
            (0x2490, 1)
        ]
    );
}

#[test]
fn context_setup_ack_is_visible_to_poll_and_unknown_requests_fail() {
    let mut mme = MacroUpload::default();
    upload(
        &mut mme,
        0,
        0,
        &[
            (0x1d00 << 14) | 0x251,
            0x331,
            0x1041,
            0x1841,
            (0x8c4 << 14) | 0x21,
            0x841,
            (0xd00 << 14) | 0x215,
            (0x40 << 14) | 0x21,
            (2 << 11) | 0xc1,
            0x11,
        ],
    );
    assert_eq!(
        mme.run(0, 0x418800, &[1, 1], |_| Ok(0)).unwrap(),
        [
            (0x3400, 0),
            (0x3404, 1),
            (0x3408, 1),
            (0x2310, 0x418800),
            (0x3400, 1),
            (0x100, 1)
        ]
    );
    assert!(mme.run(0, 0x418804, &[1, 1], |_| Ok(0)).is_err());
    assert!(mme.run(0, 0x418800, &[2, 1], |_| Ok(0)).is_err());
}
