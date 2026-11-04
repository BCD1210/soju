use super::tests::{Available, commands};
use super::*;
fn offset(x: usize, y: usize, width: usize, block_log: u32) -> usize {
    let pitch = (width * 4).div_ceil(64);
    let gobs = 1 << block_log;
    (y / (8 * gobs)) * 512 * gobs * pitch
        + (x / 64) * 512 * gobs
        + ((y % (8 * gobs)) / 8) * 512
        + ((x % 64) / 32) * 256
        + ((y % 8) / 2) * 64
        + ((x % 32) / 16) * 32
        + (y % 2) * 16
        + x % 16
}
#[test]
fn surface_clear_and_copy_match_texel_layout_and_preserve_padding() {
    for (width, height, log) in [(32_usize, 24_usize, 2_u32), (1280, 720, 4)] {
        let span = width * 4 * height.div_ceil(8 * (1 << log)) * 8 * (1 << log);
        let mut image = vec![0x55; span];
        for (address, bytes) in surface::clear(
            0x10000,
            u64::try_from(width).unwrap(),
            u64::try_from(height).unwrap(),
            log,
            [1, 2, 3, 4],
        )
        .unwrap()
        {
            let start = usize::try_from(address - 0x10000).unwrap();
            image[start..start + bytes.len()].copy_from_slice(&bytes);
        }
        let mut expected = vec![0x55; span];
        for y in 0..height {
            for x in 0..width {
                let at = offset(x * 4, y, width, log);
                expected[at..at + 4].copy_from_slice(&[1, 2, 3, 4]);
            }
        }
        assert_eq!(image, expected);
    }
    let chunks = surface::copy(0x20000, 0x10000, 301, 76, 1204, 4).unwrap();
    let mut image = vec![0x55; 1216 * 128];
    let mut expected = image.clone();
    let source: Vec<_> = (0..301 * 76 * 4)
        .map(|i| u8::try_from(i % 251).unwrap())
        .collect();
    for (src, dst, n) in chunks {
        let src = usize::try_from(src - 0x20000).unwrap();
        let dst = usize::try_from(dst - 0x10000).unwrap();
        image[dst..dst + n].copy_from_slice(&source[src..src + n]);
    }
    for y in 0..76 {
        for x in 0..301 * 4 {
            expected[offset(x, y, 301, 4)] = source[y * 1204 + x];
        }
    }
    assert_eq!(image, expected);
    assert!(surface::clear(0, 16, 7, 0, [0; 4]).is_err());
    assert!(surface::clear((1 << 40) - 4, 16, 8, 0, [0; 4]).is_err());
    assert!(surface::copy(0, 0, 301, 76, 1203, 4).is_err());
    assert!(surface::copy(u64::MAX, 0, 1, 1, 4, 0).is_err());
}
#[test]
fn executor_clear_and_copy_validate_profiles_without_executing_memory() {
    let state = ExecutionState::default();
    let mut clear = commands(0, &[(0, 0xb197)]);
    clear.extend(commands(
        0,
        &[
            (0x800, 2),
            (0x804, 0x44b0000),
            (0x808, 1280),
            (0x80c, 720),
            (0x810, 0xd5),
            (0x814, 0x40),
            (0x818, 1),
            (0x820, 0),
            (0xff4, 0x05000000),
            (0xff8, 0x02d00000),
            (0x10f8, 0),
            (0x121c, 1),
            (0x15d0, 0),
            (0x1558, 1),
            (0x19e0, 0),
            (0xd80, 0),
            (0xd84, 0),
            (0xd88, 0),
            (0xd8c, 0x3f800000),
            (0x19d0, 0x3c),
        ],
    ));
    let staged = state.stage(&clear, &Available).unwrap();
    assert_eq!(staged.pending_actions.len(), 81);
    assert!(
        matches!(&staged.pending_actions[0],Action::Write{address:0x2044b0000,bytes,kind:0xfe} if bytes[..4]==[0,0,0,255])
    );
    let copy = commands(
        4,
        &[
            (0, 0xb0b5),
            (0x708, 0x03303210),
            (0x70c, 0x1040),
            (0x710, 301),
            (0x714, 76),
            (0x718, 1),
            (0x71c, 0),
            (0x720, 0),
            (0x400, 2),
            (0x404, 0x5410000),
            (0x408, 2),
            (0x40c, 0x53e0000),
            (0x410, 1204),
            (0x414, 1216),
            (0x418, 301),
            (0x41c, 76),
            (0x300, 0x686),
        ],
    );
    let staged = state.stage(&copy, &Available).unwrap();
    match &staged.pending_actions[0] {
        Action::Copy { chunks } => {
            assert_eq!(chunks[0], (0x205410000, 0x2053e0000, 16));
            assert_eq!(chunks.iter().map(|c| c.2).sum::<usize>(), 301 * 76 * 4);
        }
        other => panic!("unexpected action {other:?}"),
    }
    assert_eq!(state, ExecutionState::default());
    struct Deny;
    impl Access for Deny {
        fn check(&self, _: u64, _: usize, _: u32, _: u8) -> bool {
            false
        }
    }
    assert!(state.stage(&clear, &Deny).is_err());
    assert!(state.stage(&copy, &Deny).is_err());
    assert_eq!(state, ExecutionState::default());
}
