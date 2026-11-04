use super::super::metal::Renderer;
use super::*;
#[test]
fn metal_clear_stages_native_pixels_without_publishing_guest_memory() {
    let mut f = Fixture::new();
    let (fd, space, va) = f.setup();
    let target = va + 0x10000;
    let values = [
        (0, 0xb197),
        (0x800, (target >> 32) as u32),
        (0x804, target as u32),
        (0x808, 32),
        (0x80c, 32),
        (0x810, 0xd5),
        (0x814, 0),
        (0x818, 1),
        (0x121c, 1),
        (0xd7c, 1),
        (0xd80, 1f32.to_bits()),
        (0xd84, 0),
        (0xd88, 0),
        (0xd8c, 1f32.to_bits()),
        (0x11c, 0),
        (0x120, 0),
        (0x114, 0),
        (0x118, ((0x19d0 / 4) << 14) | 0x21),
        (0x118, 0x841),
        (0x118, 0x91),
        (0x118, 0x11),
        (0x3800, 60),
        (0x2c8, (1 << 20) | fd),
        (0x2c8, (1 << 20) | (1 << 16) | fd),
    ];
    let methods: Vec<_> = values
        .into_iter()
        .map(|(method, value)| gpu::Method {
            address: va,
            subchannel: 0,
            method,
            value,
        })
        .collect();
    let access = submit::Access {
        space: &f.nv.spaces[&space],
        memory: &f.memory,
    };
    let (writes, increments) = Renderer::default()
        .submit(&methods, fd, Some(2), &access, &[])
        .unwrap();
    assert_eq!(increments, 2);
    let mut clipped = methods.clone();
    let draw = clipped.iter().position(|m| m.method == 0x3800).unwrap();
    clipped.insert(
        draw,
        gpu::Method {
            address: va,
            subchannel: 0,
            method: 0x194c,
            value: 1,
        },
    );
    assert!(
        Renderer::default()
            .submit(&clipped, fd, Some(2), &access, &[])
            .is_err()
    );
    assert!(!writes.is_empty());
    let mut before = vec![0; 4096];
    f.memory
        .read(crate::kernel::HEAP + 0x10000, &mut before)
        .unwrap();
    assert!(before.iter().all(|b| *b == 0));
    for write in writes {
        f.memory.write(write.address, &write.bytes).unwrap();
    }
    f.memory
        .read(crate::kernel::HEAP + 0x10000, &mut before)
        .unwrap();
    assert!(before.chunks_exact(4).all(|p| p == [255, 0, 0, 255]));
    assert_eq!(f.nv.channels[&fd].completed, 0);
    let access = submit::Access {
        space: &f.nv.spaces[&space],
        memory: &f.memory,
    };
    let mut renderer = Renderer::default();
    let bad = [gpu::Method {
        address: va,
        subchannel: 0,
        method: 4,
        value: 0,
    }];
    assert!(renderer.submit(&bad, fd, None, &access, &[]).is_err());
    assert!(
        renderer
            .submit(&methods, fd, Some(2), &access, &[])
            .is_err()
    );
}

fn methods(values: &[(u32, u32)]) -> Vec<gpu::Method> {
    values
        .iter()
        .map(|&(method, value)| gpu::Method {
            address: 0,
            subchannel: 0,
            method,
            value,
        })
        .collect()
}

#[test]
fn metal_point_center_and_watchdog_validate_fields_without_completion() {
    let mut f = Fixture::new();
    let (fd, space, _) = f.setup();
    let access = submit::Access {
        space: &f.nv.spaces[&space],
        memory: &f.memory,
    };
    for (offset, valid, invalid) in [(0x165c, 1, 2), (0xde4, 63, 64)] {
        let (writes, count) = Renderer::default()
            .submit(
                &methods(&[(0, 0xb197), (offset, valid)]),
                fd,
                None,
                &access,
                &[],
            )
            .unwrap();
        assert!(writes.is_empty());
        assert_eq!(count, 0);
        assert!(
            Renderer::default()
                .submit(
                    &methods(&[(0, 0xb197), (offset, invalid)]),
                    fd,
                    None,
                    &access,
                    &[]
                )
                .is_err()
        );
    }
}

#[test]
fn metal_initialization_has_no_invented_fence_and_checks_declared_increments() {
    let mut f = Fixture::new();
    let (fd, space, _) = f.setup();
    let access = submit::Access {
        space: &f.nv.spaces[&space],
        memory: &f.memory,
    };
    let init = methods(&[(0, 0x902d), (0x260, 0), (0x2a4, 0x7f800000), (0x884, 0x3f)]);
    let (writes, count) = Renderer::default()
        .submit(&init, fd, None, &access, &[])
        .unwrap();
    assert!(writes.is_empty());
    assert_eq!(count, 0);
    let mut renderer = Renderer::default();
    assert!(renderer.submit(&init, fd, Some(1), &access, &[]).is_err());
    assert!(renderer.submit(&init, fd, None, &access, &[]).is_err());
    let bad = methods(&[(0, 0xb197), (0x2c8, (1 << 20) | (fd + 1))]);
    assert!(
        Renderer::default()
            .submit(&bad, fd, None, &access, &[])
            .is_err()
    );
}

#[test]
fn metal_twod_copy_obeys_initialization_and_stages_pixels() {
    let mut f = Fixture::new();
    let (fd, space, va) = f.setup();
    let src = va + 0x10000;
    let dst = va + 0x20000;
    let pixels: Vec<_> = (0..16).collect();
    f.memory
        .write(crate::kernel::HEAP + 0x10000, &pixels)
        .unwrap();
    let values = [
        (0, 0x902d),
        (0x260, 0),
        (0x26c, 1),
        (0x290, 0),
        (0x29c, 0),
        (0x2a4, 0x7f800000),
        (0x2a8, u32::MAX),
        (0x884, 0x3f),
        (0x88c, 0),
        (0x200, 0xd5),
        (0x204, 1),
        (0x208, 0),
        (0x20c, 1),
        (0x210, 0),
        (0x214, 8),
        (0x218, 2),
        (0x21c, 2),
        (0x220, (dst >> 32) as u32),
        (0x224, dst as u32),
        (0x230, 0xd5),
        (0x234, 1),
        (0x238, 0),
        (0x23c, 1),
        (0x244, 8),
        (0x248, 2),
        (0x24c, 2),
        (0x250, (src >> 32) as u32),
        (0x254, src as u32),
        (0x2ac, 3),
        (0x8b0, 0),
        (0x8b4, 0),
        (0x8b8, 2),
        (0x8bc, 2),
        (0x8c0, 0),
        (0x8c4, 1),
        (0x8c8, 0),
        (0x8cc, 1),
        (0x8d0, 0),
        (0x8d4, 0),
        (0x8d8, 0),
        (0x8dc, 0),
    ];
    let access = submit::Access {
        space: &f.nv.spaces[&space],
        memory: &f.memory,
    };
    let (writes, count) = Renderer::default()
        .submit(&methods(&values), fd, None, &access, &[])
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].address, crate::kernel::HEAP + 0x20000);
    assert_eq!(&writes[0].bytes[..16], pixels);
    let mut unchanged = [1; 16];
    f.memory
        .read(crate::kernel::HEAP + 0x20000, &mut unchanged)
        .unwrap();
    assert_eq!(unchanged, [0; 16]);
    for setting in [0x290, 0x29c, 0x2b8, 0x26c, 0x88c] {
        let mut bad = methods(&values[..values.len() - 1]);
        bad.extend(methods(&[
            (
                setting,
                if setting == 0x26c {
                    2
                } else if setting == 0x88c {
                    0x10
                } else {
                    1
                },
            ),
            (0x8dc, 0),
        ]));
        assert!(
            Renderer::default()
                .submit(&bad, fd, None, &access, &[])
                .is_err()
        );
    }
}
