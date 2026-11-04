use super::tests::{Harness, req};
use super::*;
use crate::nro::TLS;
#[test]
fn invalid_storage_and_player_requests_preserve_state() {
    let mut h = Harness::new();
    let root = h.open("appletAE");
    let proxy = h.child(root, 200);
    let creator = h.child(proxy, 11);
    for size in [4097, u64::MAX] {
        assert!(
            h.call(creator, req(10, size.to_le_bytes().to_vec()))
                .is_err()
        );
    }
    assert!(h.call(creator, req(0, vec![0; 8])).is_err());
    let storage = h
        .call(creator, req(10, 8u64.to_le_bytes().to_vec()))
        .unwrap()
        .objects[0];
    let accessor = h.child(storage, 0);
    assert_eq!(
        h.call(accessor, req(0, vec![])).unwrap().data,
        8u64.to_le_bytes()
    );
    for id in [10, 11] {
        assert!(h.call(accessor, req(id, vec![0; 8])).is_err());
    }
    for offset in [9, u64::MAX] {
        let mut r = req(11, offset.to_le_bytes().to_vec());
        r.buffers.push(Buffer {
            kind: BufferKind::ReceiveList,
            address: TLS + 4096,
            len: 1,
            index: 0,
        });
        assert!(h.call(accessor, r).is_err());
    }
    let player = h
        .call(
            creator,
            req(0, [0x10u32.to_le_bytes(), 0u32.to_le_bytes()].concat()),
        )
        .unwrap()
        .objects[0];
    assert!(h.call(player, req(30, vec![])).is_err());
    assert!(h.call(player, req(100, vec![])).is_err());
    let mut r = req(100, vec![]);
    r.objects.push(h.k.object_id(accessor).unwrap());
    assert!(h.call(player, r).is_err());
    let id = h.k.object_id(storage).unwrap();
    h.p.objects.get_mut(&id).unwrap().client = ClientId(2);
    let mut r = req(100, vec![]);
    r.objects.push(id);
    assert!(h.call(player, r).is_err());
    assert!(
        h.p.players
            .get(&h.k.object_id(player).unwrap())
            .unwrap()
            .inputs
            .is_empty()
    );
}
#[test]
fn calendar_handles_leap_day_negative_epoch_and_range() {
    let mut h = Harness::new();
    let time = h.open("time:u");
    let zone = h.child(time, 3);
    for (stamp, expected, day) in [
        (-1i64, [0xb1, 7, 12, 31, 23, 59, 59, 0], 364u32),
        (951782400, [0xd0, 7, 2, 29, 0, 0, 0, 0], 59),
    ] {
        let data = h
            .call(zone, req(101, stamp.to_le_bytes().to_vec()))
            .unwrap()
            .data;
        assert_eq!(&data[..8], &expected);
        assert_eq!(&data[12..16], &day.to_le_bytes());
    }
    for stamp in [i64::MIN, -62135596801, 253402300800, i64::MAX] {
        assert!(
            h.call(zone, req(101, stamp.to_le_bytes().to_vec()))
                .is_err()
        );
    }
}
#[test]
fn fonts_reject_missing_short_tls_and_exchange_outputs() {
    let mut h = Harness::new();
    h.p.fonts = Some(fonts::fixture());
    let pl = h.open("pl:u");
    assert!(h.call(pl, req(5, vec![0; 8])).is_err());
    for (kind, address, len) in [
        (BufferKind::Receive, TLS, 24),
        (BufferKind::Receive, TLS + 4096, 23),
        (BufferKind::Exchange, TLS + 4096, 24),
    ] {
        let mut r = req(5, vec![0; 8]);
        for i in 0..3u16 {
            r.buffers.push(Buffer {
                kind,
                address: address + u64::from(i) * 32,
                len,
                index: i,
            });
        }
        assert!(h.call(pl, r).is_err());
    }
    let firmware = h.open("set:sys");
    let mut r = req(4, vec![]);
    r.buffers.push(Buffer {
        kind: BufferKind::ReceiveList,
        address: TLS + 4096,
        len: 257,
        index: 0,
    });
    assert!(h.call(firmware, r).is_err());
}
#[test]
fn closing_event_owner_keeps_guest_alias_valid() {
    let mut h = Harness::new();
    let ae = h.open("appletAE");
    let proxy = h.child(ae, 200);
    let common = h.child(proxy, 0);
    let original = h.call(common, req(0, vec![])).unwrap().copy_handles[0];
    let copy = h.k.duplicate_handle(original).unwrap();
    h.p.close_object(h.k.object_id(common).unwrap());
    h.p.poll(&mut h.k, &mut h.m, 0).unwrap();
    assert!(h.k.object_id(original).is_err());
    assert!(h.k.object_id(copy).is_ok());
    assert!(!h.p.request_exit(&mut h.k).unwrap());
}
