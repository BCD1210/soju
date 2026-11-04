use soju_switch_gpu::{
    engine::{Engine, TWOD},
    memory::Memory,
};
use soju_switch_texture::Surface;

const WIDTH: usize = 37;
const HEIGHT: usize = 19;
const PITCH: usize = 160;

fn surface(block: u32) -> Surface {
    Surface {
        width: WIDTH,
        height: HEIGHT,
        depth: 1,
        bytes_per_block: 4,
        block_height_log2: block,
        block_depth_log2: 0,
    }
}
fn offset(layout: Option<u32>, x: usize, y: usize) -> usize {
    layout.map_or(y * PITCH + x * 4, |b| surface(b).offset(x * 4, y, 0))
}
fn size(layout: Option<u32>) -> usize {
    layout.map_or(PITCH * HEIGHT, |b| surface(b).size())
}
fn configure(e: &mut Engine, m: &mut Memory, src: Option<u32>, dst: Option<u32>, alias: bool) {
    for (base, layout, address) in [
        (0x230, src, 0x1000),
        (0x200, dst, if alias { 0x1000 } else { 0x10000 }),
    ] {
        for (r, v) in [
            (0, 0xd5),
            (4, u32::from(layout.is_none())),
            (8, layout.unwrap_or(0) << 4),
            (12, 1),
            (0x14, PITCH as u32),
            (0x18, WIDTH as u32),
            (0x1c, HEIGHT as u32),
            (0x24, address),
        ] {
            e.method(TWOD, base + r, v, m).unwrap();
        }
    }
    for (r, v) in [
        (0x2ac, 3),
        (0x8b0, 5),
        (0x8b4, 3),
        (0x8b8, 29),
        (0x8bc, 15),
        (0x8c4, 1),
        (0x8cc, 1),
        (0x8d4, 2),
    ] {
        e.method(TWOD, r, v, m).unwrap();
    }
}

#[test]
fn copy_rect_preserves_padding_across_layouts_and_gob_boundaries() {
    let layouts = [None, Some(0), Some(1), Some(2), Some(3), Some(4), Some(5)];
    for src in layouts {
        for dst in layouts {
            for alias in [false, true] {
                let mut m = Memory::default();
                let mut e = Engine::default();
                let mut source = vec![0xa5; size(src).max(if alias { size(dst) } else { 0 })];
                for y in 0..HEIGHT {
                    for x in 0..WIDTH {
                        let o = offset(src, x, y);
                        source[o..o + 4].copy_from_slice(&[x as u8, y as u8, (x ^ y) as u8, 255]);
                    }
                }
                let mut expected = if alias {
                    source.clone()
                } else {
                    vec![0x5a; size(dst)]
                };
                for y in 0..15 {
                    for x in 0..29 {
                        let from = offset(src, x + 2, y + 1);
                        let to = offset(dst, x + 5, y + 3);
                        expected[to..to + 4].copy_from_slice(&source[from..from + 4]);
                    }
                }
                m.map(0x1000, source).unwrap();
                if !alias {
                    m.map(0x10000, vec![0x5a; size(dst)]).unwrap();
                }
                configure(&mut e, &mut m, src, dst, alias);
                e.method(TWOD, 0x8dc, 1, &mut m).unwrap();
                assert_eq!(
                    m.read(if alias { 0x1000 } else { 0x10000 }, expected.len())
                        .unwrap(),
                    expected,
                    "src={src:?} dst={dst:?} alias={alias}"
                );
            }
        }
    }
}

#[test]
fn invalid_copy_leaves_destination_and_generation_unchanged() {
    for (register, value) in [
        (0x208, 0x100),
        (0x208, 0x60),
        (0x204, 2),
        (0x218, 0),
        (0x8b8, u32::MAX),
        (0x224, 0xfffffff0),
        (0x254, 0xfffffff0),
    ] {
        let mut m = Memory::default();
        let mut e = Engine::default();
        m.map(0x1000, vec![0xa5; size(Some(1))]).unwrap();
        m.map(0x10000, vec![0x5a; size(Some(2))]).unwrap();
        configure(&mut e, &mut m, Some(1), Some(2), false);
        let revision = m.revision();
        e.method(TWOD, register, value, &mut m).unwrap();
        assert!(e.method(TWOD, 0x8dc, 1, &mut m).is_err());
        assert_eq!(m.revision(), revision);
        assert!(
            m.read(0x10000, size(Some(2)))
                .unwrap()
                .iter()
                .all(|b| *b == 0x5a)
        );
    }
}

#[test]
fn bilinear_half_texel_reduction_averages_four_samples_and_preserves_other_bytes() {
    for src in [None, Some(0), Some(1), Some(4)] {
        for dst in [None, Some(0), Some(2), Some(4)] {
            for alias in [false, true] {
                let mut m = Memory::default();
                let mut e = Engine::default();
                let mut source = vec![0xa5; size(src).max(if alias { size(dst) } else { 0 })];
                for y in 0..HEIGHT {
                    for x in 0..WIDTH {
                        let o = offset(src, x, y);
                        source[o..o + 4].copy_from_slice(&[
                            (x * 7) as u8,
                            (y * 17) as u8,
                            (x * 11 + y * 13) as u8,
                            (x ^ y) as u8,
                        ]);
                    }
                }
                let mut expected = if alias {
                    source.clone()
                } else {
                    vec![0x5a; size(dst)]
                };
                for y in 0..7 {
                    for x in 0..9 {
                        for c in 0..4 {
                            let mut sum = 0u16;
                            for yy in 0..2 {
                                for xx in 0..2 {
                                    sum += u16::from(
                                        source[offset(src, 15 + x * 2 + xx, 3 + y * 2 + yy) + c],
                                    );
                                }
                            }
                            expected[offset(dst, 5 + x, 3 + y) + c] = ((sum + 2) / 4) as u8;
                        }
                    }
                }
                m.map(0x1000, source).unwrap();
                if !alias {
                    m.map(0x10000, vec![0x5a; size(dst)]).unwrap();
                }
                configure(&mut e, &mut m, src, dst, alias);
                for (r, v) in [
                    (0x88c, 0x10),
                    (0x8b8, 9),
                    (0x8bc, 7),
                    (0x8c4, 2),
                    (0x8cc, 2),
                    (0x8d0, 0x80000000),
                    (0x8d4, 15),
                    (0x8d8, 0x80000000),
                    (0x8dc, 3),
                ] {
                    e.method(TWOD, r, v, &mut m).unwrap();
                }
                let address = if alias { 0x1000 } else { 0x10000 };
                assert_eq!(m.read(address, expected.len()).unwrap(), expected);
                let revision = m.revision();
                e.method(TWOD, 0x88c, 0x11, &mut m).unwrap();
                assert!(e.method(TWOD, 0x8dc, 3, &mut m).is_err());
                assert_eq!(m.revision(), revision);
                assert_eq!(m.read(address, expected.len()).unwrap(), expected);
            }
        }
    }
}

#[test]
fn srgb_reduction_linearizes_rgb_but_keeps_alpha_linear() {
    let mut m = Memory::default();
    let mut e = Engine::default();
    let mut source = vec![0; PITCH * HEIGHT];
    // Each 2x2 group has two black and two white color samples; alpha is linear.
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let value = if x % 2 == 0 { 0 } else { 255 };
            source[y * PITCH + x * 4..y * PITCH + x * 4 + 4].fill(value);
        }
    }
    m.map(0x1000, source).unwrap();
    m.map(0x10000, vec![0x5a; PITCH * HEIGHT]).unwrap();
    configure(&mut e, &mut m, None, None, false);
    for (r, v) in [
        (0x200, 0xd6),
        (0x230, 0xd6),
        (0x88c, 0x10),
        (0x8b8, 9),
        (0x8bc, 7),
        (0x8c4, 2),
        (0x8cc, 2),
        (0x8d0, 0x80000000),
        (0x8d4, 2),
        (0x8d8, 0x80000000),
        (0x8dc, 1),
    ] {
        e.method(TWOD, r, v, &mut m).unwrap();
    }
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let expected = if (5..14).contains(&x) && (3..10).contains(&y) {
                [188, 188, 188, 128]
            } else {
                [0x5a; 4]
            };
            assert_eq!(
                m.read(0x10000 + (y * PITCH + x * 4) as u64, 4).unwrap(),
                expected
            );
        }
    }
}
