use super::*;
#[test]
fn nvmap_lifetime_generation_and_session_ownership() {
    let mut f = Fixture::new();
    let fd = f.open("/dev/nvmap");
    let id = f.allocation(fd, 4096);
    let view = f.nv.resolve(ClientId(1), id).unwrap();
    assert_eq!(
        (view.guest_address, view.size, view.alignment),
        (crate::kernel::HEAP, 4096, 4096)
    );
    assert!(f.nv.resolve(ClientId(2), id).is_err());
    let mut d = vec![0; 8];
    put32(&mut d, 4, id);
    assert_eq!(
        read_u32(&f.ioctl(fd, 0xc008010e, &d, 8).unwrap(), 0).unwrap(),
        id
    );
    let original = f.session;
    f.session = f.kernel.register_session(Family::Nv.code()).unwrap();
    f.nv.sessions.insert(f.kernel.object_id(f.session).unwrap());
    assert!(f.ioctl(fd, 0xc008010e, &d, 8).is_err());
    f.session = original;
    let mut d = vec![0; 24];
    put32(&mut d, 0, id);
    let out = f.ioctl(fd, 0xc0180105, &d, 24).unwrap();
    assert_eq!(read_u64(&out, 8).unwrap(), crate::kernel::HEAP);
    assert_eq!(read_u32(&out, 16).unwrap(), 4096);
    assert_eq!(read_u32(&out, 20).unwrap(), 1);
    assert!(f.nv.resolve(ClientId(1), id).is_err());
    let replacement = f.allocation(fd, 4096);
    assert_eq!(replacement, id);
    assert_ne!(
        f.nv.resolve(ClientId(1), id).unwrap().generation,
        view.generation
    );
    let cloned = f.kernel.duplicate_handle(original).unwrap();
    f.session = cloned;
    let mut d = vec![0; 8];
    put32(&mut d, 4, id);
    assert!(f.ioctl(fd, 0xc008010e, &d, 8).is_ok());
}
#[test]
fn metadata_replies_and_all_control_ioctl_profiles() {
    let mut f = Fixture::new();
    let fd = f.open("/dev/nvhost-ctrl-gpu");
    let mut d = vec![0; 176];
    put64(&mut d, 0, 160);
    put64(&mut d, 8, 1);
    let out = f.ioctl(fd, 0xc0b04705, &d, 176).unwrap();
    assert_eq!(read_u32(&out, 16).unwrap(), 0x120);
    assert_eq!(read_u64(&out, 160).unwrap(), 0x6230326d67);
    assert_eq!(
        f.ioctl(fd, 0x80044701, &[], 4).unwrap(),
        4096u32.to_le_bytes()
    );
    assert_eq!(
        read_u32(&f.ioctl(fd, 0x80284702, &[], 40).unwrap(), 24).unwrap(),
        0xc0
    );
    let mut masks = vec![0; 24];
    put32(&mut masks, 0, 4);
    put64(&mut masks, 8, 1);
    let out = f.ioctl(fd, 0xc0184706, &masks, 24).unwrap();
    assert_eq!(&out[16..], &[3, 0, 0, 0, 0, 0, 0, 0]);
    put32(&mut masks, 0, 9);
    assert!(f.ioctl(fd, 0xc0184706, &masks, 24).is_err());
    let (channel, space, va) = f.setup();
    let mut d = vec![0; 16];
    put32(&mut d, 0, 0xb197);
    assert_eq!(
        read_u64(&f.ioctl(channel, 0xc0104809, &d, 16).unwrap(), 8).unwrap(),
        1
    );
    let mut d = vec![0; 24];
    put32(&mut d, 16, 1);
    assert_eq!(f.ioctl(channel, 0xc018480c, &d, 24).unwrap(), d);
    f.ioctl(channel, 0x4004480d, &150u32.to_le_bytes(), 0)
        .unwrap();
    assert!(
        f.ioctl(channel, 0x4004480d, &151u32.to_le_bytes(), 0)
            .is_err()
    );
    let mut d = vec![0; 16];
    put64(&mut d, 0, va);
    put32(&mut d, 8, 2);
    assert_eq!(f.ioctl(channel, 0xc010480b, &d, 16).unwrap(), d);
    let mut d = vec![0; 24];
    put32(&mut d, 0, 65536);
    put32(&mut d, 4, 65536);
    put64(&mut d, 16, 65536);
    assert!(read_u64(&f.ioctl(space, 0xc0184102, &d, 24).unwrap(), 16).unwrap() >= va + 0x40000);
    let mut payload = channel.to_le_bytes().to_vec();
    payload.extend(3u32.to_le_bytes());
    let reply = f.call(&Fixture::request(4, payload)).unwrap();
    assert_eq!(reply.copy_handles.len(), 1);
    let ctrl = f.open("/dev/nvhost-ctrl");
    assert!(f.ioctl(ctrl, 0x80044701, &[], 4).is_err());
}
#[test]
fn malformed_descriptors_outputs_and_mapped_free_preserve_state() {
    let mut f = Fixture::new();
    let fd = f.open("/dev/nvmap");
    let mut d = vec![0; 8];
    put32(&mut d, 0, 4096);
    let mut r = f.ioctl_request(fd, 0xc0080101, &d, 8);
    r.buffers.remove(0);
    assert!(f.call(&r).is_err());
    assert!(f.nv.allocations.is_empty());
    assert!(f.ioctl(fd, 0xc0080101, &d, 7).is_err());
    assert!(f.nv.allocations.is_empty());
    let (channel, space, va) = f.setup();
    let nvmap = f.nv.channels[&channel].nvmap.unwrap();
    let id = f.nv.spaces[&space].mappings[&va].view.allocation;
    let mut d = vec![0; 24];
    put32(&mut d, 0, id);
    assert!(f.ioctl(nvmap, 0xc0180105, &d, 24).is_err());
    assert!(f.nv.allocations.contains_key(&id));
    let mut d = vec![0; 40];
    put32(&mut d, 4, 0xfe);
    put32(&mut d, 8, id);
    put32(&mut d, 12, 65536);
    let tiled = read_u64(&f.ioctl(space, 0xc0284106, &d, 40).unwrap(), 32).unwrap();
    put32(&mut d, 0, 4);
    put32(&mut d, 4, 0);
    put32(&mut d, 12, 131072);
    let cached = read_u64(&f.ioctl(space, 0xc0284106, &d, 40).unwrap(), 32).unwrap();
    assert_eq!(cached % 131072, 0);
    assert_eq!(
        f.nv.spaces[&space].resolve(cached, 4).unwrap(),
        (crate::kernel::HEAP, 0)
    );
    let a = &f.nv.spaces[&space];
    assert_eq!(a.resolve(va, 4).unwrap(), (crate::kernel::HEAP, 0));
    assert_eq!(a.resolve(tiled, 4).unwrap(), (crate::kernel::HEAP, 0xfe));
    assert_ne!(va, tiled);
    f.call(&Fixture::request(2, space.to_le_bytes().to_vec()))
        .unwrap();
    assert!(f.nv.spaces.contains_key(&space));
    f.call(&Fixture::request(2, channel.to_le_bytes().to_vec()))
        .unwrap();
    assert!(!f.nv.spaces.contains_key(&space));
}

#[test]
fn color_memory_kind_is_retained_in_the_virtual_mapping() {
    let mut f = Fixture::new();
    let (_, space, va) = f.setup();
    let allocation = f.nv.spaces[&space].mappings[&va].view.allocation;
    let mut d = vec![0; 40];
    put32(&mut d, 0, 4);
    put32(&mut d, 4, 0xdb);
    put32(&mut d, 8, allocation);
    put32(&mut d, 12, 65536);
    let reply = f.ioctl(space, 0xc0284106, &d, 40).unwrap();
    let address = read_u64(&reply, 32).unwrap();
    assert_ne!(address, va);
    assert_eq!(
        f.nv.spaces[&space].resolve(address, 4096).unwrap(),
        (crate::kernel::HEAP, 0xdb)
    );
}

#[test]
fn compressed_depth_memory_kind_is_retained_in_the_virtual_mapping() {
    let mut f = Fixture::new();
    let (_, space, va) = f.setup();
    let allocation = f.nv.spaces[&space].mappings[&va].view.allocation;
    let mut d = vec![0; 40];
    put32(&mut d, 0, 4);
    put32(&mut d, 4, 0x17);
    put32(&mut d, 8, allocation);
    put32(&mut d, 12, 65536);
    let reply = f.ioctl(space, 0xc0284106, &d, 40).unwrap();
    let address = read_u64(&reply, 32).unwrap();
    assert_ne!(address, va);
    assert_eq!(
        f.nv.spaces[&space].resolve(address, 4096).unwrap(),
        (crate::kernel::HEAP, 0x17)
    );
}

#[test]
fn modify_mapping_kind_uses_existing_backing_without_nvmap_handle() {
    let mut f = Fixture::new();
    let (_, space, va) = f.setup();
    let mut d = vec![0; 40];
    put32(&mut d, 0, 0x100);
    put32(&mut d, 4, 0xdb);
    put64(&mut d, 16, 4096);
    put64(&mut d, 24, 8192);
    put64(&mut d, 32, va);
    f.ioctl(space, 0xc0284106, &d, 40).unwrap();
    let space_fd = space;
    let space = &f.nv.spaces[&space];
    assert_eq!(space.resolve(va, 4096).unwrap(), (crate::kernel::HEAP, 0));
    assert_eq!(
        space.resolve(va + 4096, 8192).unwrap(),
        (crate::kernel::HEAP + 4096, 0xdb)
    );
    assert_eq!(
        space.resolve(va + 12288, 4096).unwrap(),
        (crate::kernel::HEAP + 12288, 0)
    );
    assert!(space.resolve(va + 4095, 2).is_err());
    put64(&mut d, 16, 12288);
    put64(&mut d, 24, 4096);
    f.ioctl(space_fd, 0xc0284106, &d, 40).unwrap();
    assert_eq!(
        f.nv.spaces[&space_fd].resolve(va + 12288, 4096).unwrap().1,
        0xdb
    );
    put64(&mut d, 16, 0);
    put64(&mut d, 24, 16384);
    put32(&mut d, 4, 0);
    f.ioctl(space_fd, 0xc0284106, &d, 40).unwrap();
    assert_eq!(
        f.nv.spaces[&space_fd].resolve(va, 16384).unwrap(),
        (crate::kernel::HEAP, 0)
    );
}

#[test]
fn invalid_mapping_modification_preserves_backing() {
    let mut f = Fixture::new();
    let (_, space, va) = f.setup();
    let before = format!("{:?}", f.nv.spaces[&space]);
    for (offset, size, base) in [
        (1, 4096, va),
        (0, 0, va),
        (0, 0x41000, va),
        (0, 4096, va + 4096),
        (u64::MAX - 4095, 8192, va),
    ] {
        let mut d = vec![0; 40];
        put32(&mut d, 0, 0x100);
        put32(&mut d, 4, 0xdb);
        put64(&mut d, 16, offset);
        put64(&mut d, 24, size);
        put64(&mut d, 32, base);
        assert!(f.ioctl(space, 0xc0284106, &d, 40).is_err());
        assert_eq!(format!("{:?}", f.nv.spaces[&space]), before);
    }
}
#[test]
fn color_and_z16_kinds_are_retained_in_the_virtual_mapping() {
    for kind in [0x1, 0x2, 0x53, 0xe0, 0xe9] {
        let mut f = Fixture::new();
        let (_, space, va) = f.setup();
        let allocation = f.nv.spaces[&space].mappings[&va].view.allocation;
        let mut d = vec![0; 40];
        put32(&mut d, 0, 4);
        put32(&mut d, 4, kind);
        put32(&mut d, 8, allocation);
        put32(&mut d, 12, 65536);
        let reply = f.ioctl(space, 0xc0284106, &d, 40).unwrap();
        let address = read_u64(&reply, 32).unwrap();
        assert_ne!(address, va);
        assert_eq!(
            f.nv.spaces[&space].resolve(address, 4096).unwrap(),
            (crate::kernel::HEAP, kind)
        );
    }
}

#[test]
fn control_event_slots_wait_only_for_committed_fences_and_close_cleanly() {
    let mut f = Fixture::new();
    let (channel, _, _) = f.setup();
    let fd = f.open("/dev/nvhost-ctrl");
    assert!(f.ioctl(fd, 0xc004001f, &64u32.to_le_bytes(), 4).is_err());
    f.ioctl(fd, 0xc004001f, &0u32.to_le_bytes(), 4).unwrap();
    assert!(f.ioctl(fd, 0xc004001f, &0u32.to_le_bytes(), 4).is_err());
    let mut query = fd.to_le_bytes().to_vec();
    query.extend(0x10000000u32.to_le_bytes());
    assert_eq!(
        f.call(&Fixture::request(4, query))
            .unwrap()
            .copy_handles
            .len(),
        1
    );
    let mut wait = vec![0; 16];
    put32(&mut wait, 0, channel);
    put32(&mut wait, 4, 2);
    assert!(f.ioctl(fd, 0xc010001e, &wait, 16).is_err());
    f.nv.channels.get_mut(&channel).unwrap().completed = 2;
    assert_eq!(f.ioctl(fd, 0xc010001e, &wait, 16).unwrap(), wait);
    let mut read = vec![0; 8];
    put32(&mut read, 0, channel);
    assert_eq!(
        read_u32(&f.ioctl(fd, 0xc0080014, &read, 8).unwrap(), 4).unwrap(),
        2
    );
    f.ioctl(fd, 0xc0040020, &0u32.to_le_bytes(), 4).unwrap();
    assert!(f.ioctl(fd, 0xc010001e, &wait, 16).is_err());
    f.ioctl(fd, 0xc004001f, &0u32.to_le_bytes(), 4).unwrap();
    f.call(&Fixture::request(2, fd.to_le_bytes().to_vec()))
        .unwrap();
    assert!(f.nv.fence_events.is_empty());
}

#[test]
fn unmap_retires_split_views_and_allows_backing_release() {
    let mut f = Fixture::new();
    let (channel, space, va) = f.setup();
    let allocation = f.nv.spaces[&space].mappings[&va].view.allocation;
    let mut modify = vec![0; 40];
    put32(&mut modify, 0, 0x100);
    put32(&mut modify, 4, 0xfe);
    put64(&mut modify, 16, 4096);
    put64(&mut modify, 24, 4096);
    put64(&mut modify, 32, va);
    f.ioctl(space, 0xc0284106, &modify, 40).unwrap();
    assert!(
        f.ioctl(space, 0xc0084105, &(va + 4096).to_le_bytes(), 8)
            .is_err()
    );
    assert!(f.ioctl(space, 0xc0084105, &va.to_le_bytes(), 7).is_err());
    assert!(f.nv.spaces[&space].resolve(va + 4096, 4).is_ok());
    assert_eq!(
        f.ioctl(space, 0xc0084105, &va.to_le_bytes(), 8).unwrap(),
        va.to_le_bytes()
    );
    for offset in [0, 4096, 8192] {
        assert!(f.nv.spaces[&space].resolve(va + offset, 4).is_err());
    }
    assert!(f.ioctl(space, 0xc0284106, &modify, 40).is_err());
    assert!(f.ioctl(space, 0xc0084105, &va.to_le_bytes(), 8).is_err());
    let nvmap = f.nv.channels[&channel].nvmap.unwrap();
    let mut free = vec![0; 24];
    put32(&mut free, 0, allocation);
    f.ioctl(nvmap, 0xc0180105, &free, 24).unwrap();
}

#[test]
fn unmap_preserves_reservation_for_fixed_remap() {
    let mut f = Fixture::new();
    let (_, space, va) = f.setup();
    let allocation = f.nv.spaces[&space].mappings[&va].view.allocation;
    let mut reserve = vec![0; 24];
    put32(&mut reserve, 0, 4);
    put32(&mut reserve, 4, 65536);
    put64(&mut reserve, 16, 65536);
    let fixed = read_u64(&f.ioctl(space, 0xc0184102, &reserve, 24).unwrap(), 16).unwrap();
    let mut map = vec![0; 40];
    put32(&mut map, 0, 1);
    put32(&mut map, 8, allocation);
    put32(&mut map, 12, 65536);
    put64(&mut map, 32, fixed);
    let mut free = vec![0; 16];
    put64(&mut free, 0, fixed);
    put32(&mut free, 8, 4);
    put32(&mut free, 12, 65536);
    for _ in 0..2 {
        f.ioctl(space, 0xc0284106, &map, 40).unwrap();
        assert!(f.ioctl(space, 0xc0104103, &free, 16).is_err());
        f.ioctl(space, 0xc0084105, &fixed.to_le_bytes(), 8).unwrap();
        assert!(f.nv.spaces[&space].resolve(fixed, 4).is_err());
        assert!(f.nv.spaces[&space].resolve(va, 4).is_ok());
    }
    put32(&mut free, 8, 3);
    assert!(f.ioctl(space, 0xc0104103, &free, 16).is_err());
    put32(&mut free, 8, 4);
    assert_eq!(f.ioctl(space, 0xc0104103, &free, 16).unwrap(), free);
    assert!(f.ioctl(space, 0xc0104103, &free, 16).is_err());
    assert!(f.ioctl(space, 0xc0284106, &map, 40).is_err());
    assert!(f.nv.spaces[&space].resolve(va, 4).is_ok());
    assert_eq!(read_u64(&f.ioctl(space, 0xc0184102, &reserve, 24).unwrap(), 16).unwrap(), fixed);
}
