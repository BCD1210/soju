use super::*;
pub(super) struct Available;
impl Access for Available {
    fn check(&self, _: u64, _: usize, _: u32, _: u8) -> bool {
        true
    }
}
struct Unavailable;
impl Access for Unavailable {
    fn check(&self, _: u64, _: usize, _: u32, _: u8) -> bool {
        false
    }
}
pub(super) fn commands(subchannel: u32, pairs: &[(u32, u32)]) -> Vec<Method> {
    pairs
        .iter()
        .map(|&(method, value)| Method {
            address: 0x1000,
            subchannel,
            method,
            value,
        })
        .collect()
}
#[test]
fn packets_decode_modes_and_reject_malformed_streams() {
    let words = [
        (1 << 29) | (2 << 16) | 0x40,
        11,
        12,
        (3 << 29) | (2 << 16) | (2 << 13) | 0x50,
        13,
        14,
        (5 << 29) | (3 << 16) | 0x60,
        15,
        16,
        17,
        (4 << 29) | (99 << 16) | 0x70,
    ];
    let actual: Vec<_> = decode_packets(0x1000, &words)
        .unwrap()
        .iter()
        .map(|m| (m.subchannel, m.method, m.value))
        .collect();
    assert_eq!(
        actual,
        [
            (0, 0x100, 11),
            (0, 0x104, 12),
            (2, 0x140, 13),
            (2, 0x140, 14),
            (0, 0x180, 15),
            (0, 0x184, 16),
            (0, 0x184, 17),
            (0, 0x1c0, 99)
        ]
    );
    for words in [
        vec![(1 << 29) | (2 << 16), 1],
        vec![2 << 29],
        vec![(1 << 29) | (2 << 16) | 0xfff, 1, 2],
        vec![(4 << 29) | 0x1000],
    ] {
        assert!(decode_packets(0, &words).is_err());
    }
    assert_eq!(decode_entry(0, 0).unwrap(), None);
    assert_eq!(
        decode_entry(0x1000, (4 << 10) | 2 | 0x80000200).unwrap(),
        Some((0x200001000, 4))
    );
    for (low, high) in [
        (1, 4 << 10),
        (0x1000, 0),
        (0, 0x100),
        (0xfffffffc, 0xff | (2 << 10)),
        (0, (1_048_577 << 10)),
    ] {
        assert!(decode_entry(low, high).is_err());
    }
}
#[test]
fn staging_rolls_back_bindings_uploads_registers_and_effects() {
    let original = ExecutionState::default();
    let good = commands(
        0,
        &[
            (0, 0xb197),
            (0x114, 2),
            (0x118, 0x1234),
            (0x11c, 0),
            (0x120, 2),
            (0x2380, 256),
            (0x2384, 2),
            (0x2388, 0x380000),
            (0x238c, 0),
            (0x2390, 0x12345678),
            (0x2390, 0),
        ],
    );
    let staged = original.stage(&good, &Available).unwrap();
    assert_eq!(staged.registers[&(0, 0x238c)], 8);
    assert_eq!(
        staged.pending_actions,
        [
            Action::Write {
                address: 0x200380000,
                bytes: vec![0x78, 0x56, 0x34, 0x12],
                kind: 0
            },
            Action::Write {
                address: 0x200380004,
                bytes: vec![0; 4],
                kind: 0
            }
        ]
    );
    assert_eq!(original, ExecutionState::default());
    let mut bad = good.clone();
    bad.extend(commands(0, &[(0x2c4, 0)]));
    assert!(original.stage(&bad, &Available).is_err());
    assert!(original.stage(&good, &Unavailable).is_err());
    assert_eq!(original, ExecutionState::default());
    assert!(
        staged
            .stage(&commands(0, &[(0, 0xa140)]), &Available)
            .is_err()
    );
}
#[test]
fn inline_upload_and_syncpoint_preserve_order_and_reject_partial_data() {
    for class in [0xb1c0, 0xa140] {
        let base = ExecutionState::default();
        let setup = commands(
            1,
            &[
                (0, class),
                (0x188, 2),
                (0x18c, 0x3700c0),
                (0x180, 8),
                (0x184, 1),
                (0x1b0, 0x1001),
            ],
        );
        assert!(base.stage(&setup, &Available).is_err());
        let mut full = setup.clone();
        full.extend(commands(1, &[(0x1b4, 0x12345678), (0x1b4, 2)]));
        let staged = base.stage(&full, &Available).unwrap();
        assert_eq!(
            staged.pending_actions,
            [
                Action::Write {
                    address: 0x2003700c0,
                    bytes: vec![0x78, 0x56, 0x34, 0x12],
                    kind: 0
                },
                Action::Write {
                    address: 0x2003700c4,
                    bytes: vec![2, 0, 0, 0],
                    kind: 0
                },
                Action::Barrier
            ]
        );
        full.extend(commands(1, &[(0x1b4, 3)]));
        assert!(base.stage(&full, &Available).is_err());
    }
    let base = ExecutionState::default();
    let staged = base
        .stage(
            &commands(
                0,
                &[
                    (0, 0xb197),
                    (0x2380, 0x10000),
                    (0x2384, 2),
                    (0x2388, 0x320000),
                    (0x2410, 0xf1),
                    (0x2388, 0x330000),
                    (0x1144, 0),
                    (0x2c8, 0x100005),
                ],
            ),
            &Available,
        )
        .unwrap();
    assert_eq!(
        staged.constant_bindings[&(0, 0, 15)],
        (0x200320000, 0x10000)
    );
    assert_eq!(
        staged.pending_actions,
        [
            Action::Barrier,
            Action::Increment {
                syncpoint: 5,
                increment: 1,
                wait: false
            }
        ]
    );
    let counters = base
        .stage(
            &commands(
                1,
                &[(0, 0xb1c0), (0x248, 0x3803f), (0x248, 0x38000), (0x110, 0)],
            ),
            &Available,
        )
        .unwrap();
    assert_eq!(counters.compute_ref_counters[&(1, 63)], 0x380);
    let busy = ExecutionState {
        inflight_work: true,
        ..base
    };
    assert!(
        busy.stage(&commands(1, &[(0, 0xb1c0), (0x110, 0)]), &Available)
            .is_err()
    );
}
#[test]
fn supported_register_profiles_reject_unknown_values() {
    for (class, good, bad) in [
        (
            0x902d,
            vec![(0x260, 0), (0x2ac, 3), (0x884, 63)],
            (0x290, 2),
        ),
        (
            0xb197,
            vec![(0x1700, u32::MAX), (0x121c, 1), (0x1360, 1), (0x2100, 0x40)],
            (0x1360, 2),
        ),
    ] {
        let mut words = commands(0, &[(0, class)]);
        words.extend(commands(0, &good));
        let base = ExecutionState::default();
        let staged = base.stage(&words, &Available).unwrap();
        for (reg, value) in good {
            assert_eq!(staged.registers[&(0, reg)], value);
        }
        words.extend(commands(0, &[bad]));
        assert!(base.stage(&words, &Available).is_err());
        assert_eq!(base, ExecutionState::default());
    }
}
