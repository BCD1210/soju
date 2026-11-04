use super::*;
fn packet(words: &mut Vec<u32>, method: u32, value: u32) {
    words.push((1 << 29) | (1 << 16) | (method / 4));
    words.push(value);
}
impl Fixture {
    fn submission(&mut self, fd: u32, va: u64, words: &[u32]) -> Request {
        let raw: Vec<_> = words.iter().flat_map(|v| v.to_le_bytes()).collect();
        self.memory.write(crate::kernel::HEAP, &raw).unwrap();
        let mut args = vec![0; 24];
        put32(&mut args, 8, 1);
        put32(&mut args, 12, 0x104);
        put32(&mut args, 20, 1);
        self.memory.write(BASE, &args).unwrap();
        let mut entry = vec![0; 8];
        put32(&mut entry, 0, u32::try_from(va & 0xffffffff).unwrap());
        put32(
            &mut entry,
            4,
            u32::try_from(va >> 32).unwrap() | (u32::try_from(words.len()).unwrap() << 10),
        );
        self.memory.write(BASE + 512, &entry).unwrap();
        let mut payload = fd.to_le_bytes().to_vec();
        payload.extend(0xc018481bu32.to_le_bytes());
        let mut r = Fixture::request(11, payload);
        r.buffers = vec![
            Buffer {
                kind: BufferKind::Pointer,
                address: 0,
                len: 0,
                index: 0,
            },
            Buffer {
                kind: BufferKind::Pointer,
                address: 0,
                len: 0,
                index: 1,
            },
            Buffer {
                kind: BufferKind::Send,
                address: BASE,
                len: 24,
                index: 2,
            },
            Buffer {
                kind: BufferKind::Send,
                address: BASE + 512,
                len: 8,
                index: 3,
            },
            Buffer {
                kind: BufferKind::Receive,
                address: BASE + 4096,
                len: 24,
                index: 4,
            },
        ];
        r
    }
    fn finish(&mut self, token: u64, committed: bool, rollback_failed: bool) {
        let object = self.kernel.object_id(self.session).unwrap();
        self.nv
            .finish_transaction(
                token,
                committed,
                rollback_failed,
                &mut Context {
                    kernel: &mut self.kernel,
                    memory: &mut self.memory,
                    object,
                    session: self.session,
                    client: ClientId(1),
                    now_ns: 0,
                    tls: TLS,
                    nv: None,
                },
            )
            .unwrap();
    }
}
fn upload(fd: u32, target: u64) -> Vec<u32> {
    let mut w = vec![];
    packet(&mut w, 0, 0xb197);
    for (m, v) in [
        (0x2380, 4),
        (0x2384, u32::try_from(target >> 32).unwrap()),
        (0x2388, u32::try_from(target & 0xffffffff).unwrap()),
        (0x238c, 0),
        (0x2390, 0x44332211),
        (0x2c8, (1 << 20) | fd),
    ] {
        packet(&mut w, m, v);
    }
    w
}
#[test]
fn submission_fence_waits_for_memory_commit_and_failed_commit_discards_state() {
    let mut f = Fixture::new();
    let (fd, _, va) = f.setup();
    let before = f.nv.channels[&fd].execution.clone();
    let r = f.submission(fd, va, &upload(fd, va + 4096));
    let reply = f.call(&r).unwrap();
    assert_eq!(f.nv.channels[&fd].completed, 0);
    assert_eq!(f.nv.channels[&fd].execution, before);
    assert_eq!(reply.memory_writes.len(), 1);
    assert_eq!(reply.memory_writes[0].address, crate::kernel::HEAP + 4096);
    assert_eq!(&reply.memory_writes[0].bytes[..4], [0x11, 0x22, 0x33, 0x44]);
    assert!(reply.memory_writes[0].bytes[4..].iter().all(|&b| b == 0));
    let mut data = [0; 4];
    f.memory
        .read(crate::kernel::HEAP + 4096, &mut data)
        .unwrap();
    assert_eq!(data, [0; 4]);
    let token = reply.transaction.unwrap();
    let metal = f.nv.pending[&token].metal;
    f.finish(token, false, false);
    assert_eq!(f.nv.channels[&fd].execution, before);
    assert_eq!(f.nv.channels[&fd].completed, 0);
    if metal {
        // Native work cannot be replayed after failed publication.
        assert!(f.nv.channels[&fd].faulted);
        assert!(f.call(&r).is_err());
        f = Fixture::new();
        let (new_fd, _, new_va) = f.setup();
        assert_eq!((new_fd, new_va), (fd, va));
        let _ = f.submission(fd, va, &upload(fd, va + 4096));
    }
    let reply = f.call(&r).unwrap();
    for write in &reply.memory_writes {
        f.memory.write(write.address, &write.bytes).unwrap();
    }
    for write in &reply.writes {
        f.memory
            .write(BASE + 4096 + write.offset, &write.bytes)
            .unwrap();
    }
    assert_eq!(f.nv.channels[&fd].completed, 0);
    f.finish(reply.transaction.unwrap(), true, false);
    assert_eq!(f.nv.channels[&fd].completed, 1);
    f.memory
        .read(crate::kernel::HEAP + 4096, &mut data)
        .unwrap();
    assert_eq!(data, [0x11, 0x22, 0x33, 0x44]);
    let reply = f.call(&r).unwrap();
    f.finish(reply.transaction.unwrap(), false, true);
    assert!(f.nv.channels[&fd].faulted);
    assert_eq!(f.nv.channels[&fd].completed, 1);
    assert!(f.call(&r).is_err());
}
#[test]
fn invalid_packets_methods_control_overlap_and_increment_order_do_not_stage() {
    let mut f = Fixture::new();
    let (fd, _, va) = f.setup();
    let before = f.nv.channels[&fd].execution.clone();
    for words in [
        vec![0xe0000001],
        {
            let mut w = vec![];
            packet(&mut w, 0, 0xb197);
            packet(&mut w, 0xdead, 1);
            w
        },
        upload(fd, va),
        {
            let mut w = vec![];
            packet(&mut w, 0, 0xb197);
            packet(&mut w, 0x2c8, (1 << 20) | fd);
            packet(&mut w, 0x2380, 4);
            packet(&mut w, 0x2384, u32::try_from(va >> 32).unwrap());
            packet(&mut w, 0x2388, 4096);
            packet(&mut w, 0x238c, 0);
            packet(&mut w, 0x2390, 1);
            w
        },
    ] {
        let r = f.submission(fd, va, &words);
        assert!(f.call(&r).is_err());
        assert!(f.nv.pending.is_empty());
        assert_eq!(f.nv.channels[&fd].execution, before);
        assert_eq!(f.nv.channels[&fd].completed, 0);
    }
    let mut r = f.submission(fd, va, &upload(fd, va + 4096));
    r.buffers[4].address = TLS;
    assert!(f.call(&r).is_err());
    let mut r = f.submission(fd, va, &upload(fd, va + 4096));
    r.buffers.remove(0);
    assert!(f.call(&r).is_err());
}
#[test]
fn big_page_mapping_padding_is_inaccessible() {
    let mut space = memory::AddressSpace::default();
    let view = NvBufferView {
        allocation: 1,
        generation: 1,
        guest_address: crate::kernel::HEAP,
        size: 4096,
        alignment: 4096,
        kind: 0,
    };
    let mut d = vec![0; 40];
    put32(&mut d, 8, 1);
    put32(&mut d, 12, 65536);
    let a = space.map(view, &d).unwrap();
    let b = space.map(view, &d).unwrap();
    assert_eq!(b - a, 65536);
    assert!(space.resolve(a + 4096, 1).is_err());
    assert_eq!(space.resolve(a, 4096).unwrap().0, crate::kernel::HEAP);
}

#[test]
fn fixed_big_page_mapping_requires_vacant_reserved_span() {
    let mut space = memory::AddressSpace::default();
    let mut reservation = vec![0; 24];
    put32(&mut reservation, 0, 4);
    put32(&mut reservation, 4, 65536);
    put64(&mut reservation, 16, 65536);
    let base = space.reserve(&reservation).unwrap();
    let view = NvBufferView {
        allocation: 1,
        generation: 1,
        guest_address: crate::kernel::HEAP,
        size: 4096,
        alignment: 4096,
        kind: 0,
    };
    let mut d = vec![0; 40];
    put32(&mut d, 0, 5);
    put32(&mut d, 8, 1);
    put32(&mut d, 12, 65536);
    put64(&mut d, 32, base);
    assert_eq!(space.map(view, &d).unwrap(), base);
    assert!(space.map(view, &d).is_err());
    assert!(space.resolve(base + 4096, 1).is_err());
    for address in [base + 4096, base + 4 * 65536, u64::MAX - 65535] {
        put64(&mut d, 32, address);
        assert!(space.map(view, &d).is_err());
    }
    put64(&mut d, 32, base + 65536);
    assert_eq!(space.map(view, &d).unwrap(), base + 65536);
}
