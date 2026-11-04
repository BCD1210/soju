use super::*;
use crate::{
    memory::{MemoryImage, PAGE},
    nro::{BASE, Homebrew, LOADER_RETURN, Region, STACK, TLS},
};
pub(super) struct Harness {
    pub(super) p: Platform,
    pub(super) k: Kernel,
    pub(super) m: MemoryImage,
}
impl Harness {
    pub(super) fn new() -> Self {
        let boot = Homebrew {
            regions: vec![
                Region {
                    address: BASE,
                    bytes: vec![0; PAGE],
                    permissions: 5,
                },
                Region {
                    address: TLS,
                    bytes: vec![0; PAGE * 4],
                    permissions: 3,
                },
            ],
            pc: BASE,
            sp: STACK,
            x0: 0,
            x1: 0,
            x30: LOADER_RETURN,
            tpidrro_el0: TLS,
        };
        Self {
            p: Platform::new(Path::new("/nonexistent-platform-test-fonts")).unwrap(),
            k: Kernel::new(&boot),
            m: MemoryImage::new(&boot).unwrap(),
        }
    }
    pub(super) fn open(&mut self, name: &str) -> Handle {
        let h = self.k.register_session(0).unwrap();
        let id = self.k.object_id(h).unwrap();
        self.p
            .open(
                name,
                &mut Context {
                    kernel: &mut self.k,
                    memory: &mut self.m,
                    object: id,
                    session: h,
                    client: ClientId(1),
                    now_ns: 0,
                    nv: None,
                    tls: TLS,
                },
            )
            .unwrap()
    }
    pub(super) fn call(&mut self, h: Handle, r: Request) -> Result<Reply, Error> {
        let id = self.k.object_id(h).unwrap();
        self.p.dispatch(
            &r,
            &mut Context {
                kernel: &mut self.k,
                memory: &mut self.m,
                object: id,
                session: h,
                client: ClientId(1),
                now_ns: 2_000_000_000,
                nv: None,
                tls: TLS,
            },
        )
    }
    pub(super) fn child(&mut self, h: Handle, id: u32) -> Handle {
        self.call(h, req(id, vec![0; 8])).unwrap().objects[0]
    }
}
pub(super) fn req(command_id: u32, data: Vec<u8>) -> Request {
    Request {
        protocol: Protocol::Cmif,
        kind: MessageKind::Request,
        command_id,
        pid: None,
        buffers: vec![],
        data,
        copy_handles: vec![],
        move_handles: vec![],
        domain_objects: vec![],
        domain_target: None,
        objects: vec![],
        token: 0,
    }
}
fn buffer(kind: BufferKind, len: u64) -> Buffer {
    Buffer {
        kind,
        address: TLS + 0x1000,
        len,
        index: 0,
    }
}
#[test]
fn ae_oe_graph_and_unknown_interfaces() {
    let mut h = Harness::new();
    for (name, root, ids) in [
        (
            "appletAE",
            200,
            vec![0, 1, 2, 3, 4, 10, 11, 20, 21, 22, 23, 1000],
        ),
        (
            "appletAE",
            201,
            vec![0, 1, 2, 3, 4, 10, 11, 20, 21, 22, 23, 1000],
        ),
        ("appletOE", 0, vec![0, 1, 2, 3, 4, 10, 11, 20, 1000]),
    ] {
        let service = h.open(name);
        let proxy = h.child(service, root);
        for id in ids {
            let child = h.child(proxy, id);
            assert_ne!(h.k.object_id(child).unwrap(), h.k.object_id(proxy).unwrap());
        }
        assert!(matches!(
            h.call(proxy, req(99, vec![])),
            Err(Error::Unsupported { .. })
        ));
    }
    let settings = h.open("set");
    assert_eq!(
        h.call(settings, req(0, vec![])).unwrap().data,
        b"en-US\0\0\0"
    );
    assert!(matches!(
        h.call(settings, req(99, vec![])),
        Err(Error::Unsupported { .. })
    ));
    for name in ["ns:am2", "psm"] {
        let s = h.open(name);
        assert!(matches!(
            h.call(s, req(0, vec![])),
            Err(Error::Unsupported { .. })
        ));
    }
}
#[test]
fn storage_bounds_and_player_completion() {
    let mut h = Harness::new();
    let ae = h.open("appletAE");
    let proxy = h.child(ae, 201);
    let creator = h.child(proxy, 11);
    let player = h
        .call(
            creator,
            req(0, [0x10u32.to_le_bytes(), 0u32.to_le_bytes()].concat()),
        )
        .unwrap()
        .objects[0];
    assert!(h.call(player, req(10, vec![])).is_err());
    let event = h.call(player, req(0, vec![])).unwrap().copy_handles[0];
    for (size, bytes) in [
        (32u64, {
            let mut b = vec![0; 32];
            b[..12].copy_from_slice(
                &[
                    1u32.to_le_bytes(),
                    32u32.to_le_bytes(),
                    0x20000u32.to_le_bytes(),
                ]
                .concat(),
            );
            b
        }),
        (160, vec![0; 160]),
    ] {
        let storage = h
            .call(creator, req(10, size.to_le_bytes().to_vec()))
            .unwrap()
            .objects[0];
        let accessor = h.child(storage, 0);
        h.m.write(TLS + 0x1000, &bytes).unwrap();
        let mut write = req(10, vec![0; 8]);
        write.buffers.push(buffer(BufferKind::Pointer, size));
        h.call(accessor, write).unwrap();
        let mut read = req(11, vec![0; 8]);
        read.buffers.push(buffer(BufferKind::ReceiveList, size));
        assert_eq!(h.call(accessor, read).unwrap().writes[0].bytes, bytes);
        let mut bad = req(10, 1u64.to_le_bytes().to_vec());
        bad.buffers.push(buffer(BufferKind::Send, size));
        assert!(h.call(accessor, bad).is_err());
        let mut push = req(100, vec![]);
        push.objects.push(h.k.object_id(storage).unwrap());
        h.call(player, push).unwrap();
        h.p.close_object(h.k.object_id(storage).unwrap());
    }
    h.call(player, req(10, vec![])).unwrap();
    h.call(player, req(30, vec![])).unwrap();
    assert!(h.call(player, req(10, vec![])).is_err());
    let out = h.child(player, 101);
    let accessor = h.child(out, 0);
    let mut read = req(11, vec![0; 8]);
    read.buffers.push(buffer(BufferKind::Receive, 24));
    let reply = h.call(accessor, read).unwrap();
    assert_eq!(&reply.writes[0].bytes[8..16], &1u64.to_le_bytes());
    assert!(h.call(player, req(101, vec![])).is_err());
    let mut regs = [0; 31];
    regs[0] = u64::from(event);
    assert!(
        matches!(h.k.dispatch(&mut h.m,crate::kernel::MAIN_THREAD,0x17,&regs,BASE,0),crate::kernel::Dispatch::Return(v) if v[0]==0)
    );
}
#[test]
fn firmware_fonts_and_descriptor_rejection() {
    let mut h = Harness::new();
    let set = h.open("set:sys");
    for id in [3, 4] {
        let mut r = req(id, vec![]);
        r.buffers.push(buffer(BufferKind::ReceiveList, 256));
        let reply = h.call(set, r).unwrap();
        assert_eq!(reply.writes[0].bytes.len(), 256);
        assert_eq!(reply.writes[0].bytes[0], 17);
    }
    let pl = h.open("pl:u");
    h.p.fonts = None;
    assert!(h.call(pl, req(4, vec![])).is_err());
    h.p.fonts = Some(fonts::fixture());
    for (id, expected) in [(1, 1u32), (2, 7), (3, 4096)] {
        assert_eq!(
            h.call(pl, req(id, 1u32.to_le_bytes().to_vec()))
                .unwrap()
                .data,
            expected.to_le_bytes()
        );
    }
    assert!(h.call(pl, req(0, 6u32.to_le_bytes().to_vec())).is_err());
    h.call(pl, req(0, vec![0; 4])).unwrap();
    assert_eq!(h.call(pl, req(4, vec![])).unwrap().copy_handles.len(), 1);
    let mut r = req(5, vec![0; 8]);
    for i in 0..3u16 {
        r.buffers.push(Buffer {
            kind: BufferKind::Receive,
            address: TLS + 0x1000 + u64::from(i) * 32,
            len: 24,
            index: i,
        });
    }
    let reply = h.call(pl, r.clone()).unwrap();
    assert_eq!(reply.writes.len(), 3);
    assert_eq!(reply.data, [1, 0, 0, 0, 6, 0, 0, 0]);
    for (write, values) in reply.writes.iter().zip([
        [0u32, 1, 2, 3, 4, 5],
        [0, 4096, 4096, 4096, 4096, 4096],
        [17, 7, 7, 7, 7, 7],
    ]) {
        assert_eq!(
            write.bytes,
            values
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>()
        );
    }
    r.buffers[1].address = r.buffers[0].address;
    assert!(h.call(pl, r).is_err());
    assert!(h.call(set, req(3, vec![])).is_err());
}
