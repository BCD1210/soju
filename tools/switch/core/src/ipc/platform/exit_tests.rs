use super::tests::{Harness, req};
#[test]
fn locked_applet_gets_cleanup_grace_and_still_receives_exit_message() {
    let mut h = Harness::new();
    let ae = h.open("appletAE");
    let proxy = h.child(ae, 200);
    let controller = h.child(proxy, 1);
    let common = h.child(proxy, 0);
    h.call(common, req(0, vec![])).unwrap();
    h.call(controller, req(1, vec![])).unwrap();
    assert_eq!(h.p.exit_grace(), std::time::Duration::from_secs(15));
    assert!(h.p.request_exit(&mut h.k).unwrap());
    assert_eq!(
        h.call(common, req(1, vec![])).unwrap().data,
        4u32.to_le_bytes()
    );
    h.call(controller, req(2, vec![])).unwrap();
    assert_eq!(h.p.exit_grace(), std::time::Duration::from_secs(5));
}
