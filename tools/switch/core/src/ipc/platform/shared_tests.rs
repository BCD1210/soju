use super::tests::{Harness, req};
use super::*;
use crate::{
    kernel::{Dispatch, MAIN_THREAD},
    nro::{BASE, TLS},
};
fn svc(h: &mut Harness, id: u16, args: &[u64]) -> Vec<u64> {
    let mut regs = [0; 31];
    regs[..args.len()].copy_from_slice(args);
    match h.k.dispatch(&mut h.m, MAIN_THREAD, id, &regs, BASE, 0) {
        Dispatch::Return(v) => v,
        v => panic!("unexpected {v:?}"),
    }
}
#[test]
fn hid_and_time_map_real_shared_bytes() {
    let mut h = Harness::new();
    h.p.set_clock_epoch(1_700_000_000);
    let hid = h.open("hid");
    let resource = h.child(hid, 0);
    let shared = h.call(resource, req(0, vec![])).unwrap().copy_handles[0];
    let addr = 0x80000000;
    assert_eq!(
        svc(&mut h, 0x13, &[u64::from(shared), addr, 0x40000, 1]),
        [0]
    );
    let mut header = [0; 4];
    h.m.read(addr + 0x9a00, &mut header).unwrap();
    assert_eq!(header, 1u32.to_le_bytes());
    let buttons = (1 << 12) | (1 << 15) | (1 << 1);
    h.p.set_buttons(buttons);
    h.p.poll(&mut h.k, &mut h.m, 3_000_000_000).unwrap();
    let mut sample = [0; 48];
    h.m.read(addr + 0x9a48, &mut sample).unwrap();
    assert_eq!(&sample[..8], &2u64.to_le_bytes());
    assert_eq!(&sample[16..24], &buttons.to_le_bytes());
    assert_eq!(&sample[24..40], &[0; 16]);
    assert_eq!(&sample[40..], &[1, 0, 0, 0, 0, 0, 0, 0]);
    h.p.set_input(soju_switch_input::NpadState {
        buttons,
        left: soju_switch_input::Stick { x: -32767, y: 1234 },
        right: soju_switch_input::Stick { x: 42, y: -9000 },
    });
    h.p.poll(&mut h.k, &mut h.m, 3_000_000_000).unwrap();
    h.m.read(addr + 0x9a48, &mut sample).unwrap();
    let axes: Vec<_> = [-32767i32, 1234, 42, -9000]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    assert_eq!(&sample[24..40], axes);
    let time = h.open("time:u");
    for id in 0..3 {
        let clock = h.child(time, id);
        assert_eq!(
            h.call(clock, req(0, vec![])).unwrap().data,
            1_700_000_002i64.to_le_bytes()
        );
    }
    let timezone = h.child(time, 3);
    let date = h
        .call(timezone, req(101, 0u64.to_le_bytes().to_vec()))
        .unwrap()
        .data;
    assert_eq!(&date[..8], &[0xb2, 7, 1, 1, 0, 0, 0, 0]);
    assert_eq!(&date[8..12], &4u32.to_le_bytes());
    assert_eq!(&date[16..19], b"UTC");
    let shared = h.call(time, req(20, vec![])).unwrap().copy_handles[0];
    let addr = 0x90000000;
    assert_eq!(svc(&mut h, 0x13, &[u64::from(shared), addr, 4096, 1]), [0]);
    h.p.poll(&mut h.k, &mut h.m, 7_000_000_000).unwrap();
    for offset in [0x40, 0x60, 0x88, 0xa8] {
        let mut value = [0; 8];
        h.m.read(addr + offset, &mut value).unwrap();
        assert_eq!(value, 1_700_000_007i64.to_le_bytes());
    }
}
#[test]
fn message_event_queue_and_alias() {
    let mut h = Harness::new();
    let ae = h.open("appletAE");
    let proxy = h.child(ae, 200);
    let common = h.child(proxy, 0);
    let event = h.call(common, req(0, vec![])).unwrap().copy_handles[0];
    let alias = h.k.duplicate_handle(event).unwrap();
    assert_eq!(h.call(common, req(1, vec![])).unwrap().result, 0x680);
    assert!(h.p.request_exit(&mut h.k).unwrap());
    assert!(h.p.request_exit(&mut h.k).unwrap());
    assert_eq!(svc(&mut h, 0x17, &[u64::from(alias)]), [0]);
    h.k.signal_event(event).unwrap();
    assert_eq!(
        h.call(common, req(1, vec![])).unwrap().data,
        4u32.to_le_bytes()
    );
    assert_eq!(
        svc(&mut h, 0x17, &[u64::from(alias)]),
        [crate::kernel::INVALID_STATE]
    );
    assert_eq!(h.call(common, req(1, vec![])).unwrap().result, 0x680);
}
#[test]
fn command_inventory_and_simple_services() {
    let mut h = Harness::new();
    let hid = h.open("hid");
    for (id, len) in [
        (11, 8),
        (21, 8),
        (31, 8),
        (100, 16),
        (102, 8),
        (103, 8),
        (109, 16),
        (124, 8),
        (201, 24),
        (204, 1),
        (206, 8),
    ] {
        assert_eq!(h.call(hid, req(id, vec![0; len])).unwrap().result, 0);
        assert!(h.call(hid, req(id, vec![])).is_err());
    }
    assert_eq!(h.call(hid, req(205, vec![])).unwrap().data, [0; 8]);
    let vibration = h.child(hid, 203);
    h.call(vibration, req(0, vec![0; 4])).unwrap();
    for name in ["bsd:s", "bsd:u"] {
        let s = h.open(name);
        for id in [0, 1] {
            assert_eq!(h.call(s, req(id, vec![])).unwrap().result, 0);
        }
    }
    let account = h.open("acc:u1");
    assert!(h.call(account, req(50, vec![])).is_err());
    assert_eq!(h.call(account, req(50, vec![0; 8])).unwrap().data[0], 1);
    let apm = h.open("apm:p");
    h.call(apm, req(1, vec![])).unwrap();
    let session = h.child(apm, 0);
    assert!(h.call(session, req(0, vec![])).is_err());
    let ae = h.open("appletAE");
    let proxy = h.child(ae, 200);
    for (child, ids) in [
        (0, vec![5, 6, 9]),
        (1, vec![9, 11, 12, 40, 91]),
        (2, vec![1]),
        (10, vec![0]),
        (20, vec![11]),
    ] {
        let service = h.child(proxy, child);
        for id in ids {
            h.call(service, req(id, vec![0; 8])).unwrap();
        }
    }
    let time = h.open("time:s");
    let empty = h.child(time, 4);
    assert!(h.call(empty, req(0, vec![])).is_err());
    let id = h.k.object_id(hid).unwrap();
    assert!(
        h.p.dispatch(
            &req(205, vec![]),
            &mut Context {
                kernel: &mut h.k,
                memory: &mut h.m,
                object: id,
                session: hid,
                client: ClientId(2),
                now_ns: 0,
                tls: TLS,
                nv: None
            }
        )
        .is_err()
    );
    for kind in [
        Kind::Empty,
        Kind::Firmware,
        Kind::Ae,
        Kind::Oe,
        Kind::LibraryProxy,
        Kind::ApplicationProxy,
        Kind::Common,
        Kind::SelfController,
        Kind::Window,
        Kind::Winding,
        Kind::LibrarySelf,
        Kind::Creator,
        Kind::Storage,
        Kind::Accessor,
        Kind::Player,
        Kind::Hid,
        Kind::HidResource,
        Kind::Vibration,
        Kind::Time,
        Kind::Clock,
        Kind::Timezone,
        Kind::Account,
        Kind::Apm,
        Kind::Fonts,
        Kind::Bsd,
    ] {
        let table = tables::table(kind);
        assert!(table.windows(2).all(|pair| pair[0].id < pair[1].id));
    }
}
