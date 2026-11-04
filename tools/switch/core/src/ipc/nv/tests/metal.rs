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
        (0x19d0, 60),
        (0x2c8, (1 << 20) | fd),
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
    let writes = Renderer::default()
        .submit(&methods, fd, &access, &[])
        .unwrap();
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
    assert!(renderer.submit(&bad, fd, &access, &[]).is_err());
    assert!(renderer.submit(&methods, fd, &access, &[]).is_err());
}
