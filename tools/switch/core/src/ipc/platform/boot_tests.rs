use super::{
    tests::{Harness, req},
    *,
};

#[test]
fn application_launch_storage_is_consumed_once() {
    let mut h = Harness::new();
    let root = h.open("appletOE");
    let proxy = h.child(root, 0);
    let functions = h.child(proxy, 20);
    let missing = h
        .call(functions, req(1, 1u32.to_le_bytes().to_vec()))
        .unwrap();
    assert_ne!(missing.result, 0);
    let storage = h
        .call(functions, req(1, 2u32.to_le_bytes().to_vec()))
        .unwrap()
        .objects[0];
    let accessor = h.child(storage, 0);
    assert_eq!(
        h.call(accessor, req(0, vec![])).unwrap().data,
        0x88u64.to_le_bytes()
    );
    assert_ne!(
        h.call(functions, req(1, 2u32.to_le_bytes().to_vec()))
            .unwrap()
            .result,
        0
    );
    assert_eq!(
        h.call(functions, req(21, vec![])).unwrap().data,
        b"en-US\0\0\0"
    );
}

#[test]
fn mode_roundtrips_and_invalid_modes_do_not_change_state() {
    let mut h = Harness::new();
    let apm = h.open("apm");
    let session = h.child(apm, 0);
    let data = [1u32.to_le_bytes(), 0x10002u32.to_le_bytes()].concat();
    h.call(session, req(0, data)).unwrap();
    assert_eq!(
        h.call(session, req(1, 1u32.to_le_bytes().to_vec()))
            .unwrap()
            .data,
        0x10002u32.to_le_bytes()
    );
    assert!(
        h.call(
            session,
            req(0, [2u32.to_le_bytes(), 0u32.to_le_bytes()].concat())
        )
        .is_err()
    );
    let hid = h.open("hid");
    let mut data = vec![0; 16];
    data[0] = 31;
    h.call(hid, req(100, data)).unwrap();
    assert_eq!(
        h.call(hid, req(101, vec![0; 8])).unwrap().data,
        31u32.to_le_bytes()
    );
    let mut data = vec![0; 16];
    data[8] = 2;
    h.call(hid, req(128, data.clone())).unwrap();
    data[8] = 3;
    assert!(h.call(hid, req(128, data)).is_err());
    assert_eq!(
        h.call(hid, req(129, vec![0; 8])).unwrap().data,
        2u64.to_le_bytes()
    );
    assert!(matches!(
        h.call(hid, req(9999, vec![])),
        Err(Error::Unsupported { .. })
    ));
}
