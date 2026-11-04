//! Offline local-wireless capability profile. No wireless interface is exposed.
use super::{tables::c, *};
const NOT_SUPPORTED: u32 = 0xfe01;
const NOT_INITIALIZED: u32 = 0x4acb;
pub(super) const LDN_CREATOR: &[CommandSpec<Platform>] =
    &[c(0, 0, |s, _, x| s.child(Kind::Ldn, x))];
pub(super) const LDN: &[CommandSpec<Platform>] = &[
    c(0, 0, |_, _, _| Ok(Reply::data(0u32.to_le_bytes().to_vec()))),
    c(1, 0, not_initialized),
    c(2, 0, not_initialized),
    c(3, 0, not_initialized),
    c(4, 0, not_initialized),
    c(5, 0, not_initialized),
    c(100, 0, event),
    c(101, 0, not_initialized),
    c(102, 0, not_initialized),
    c(103, 0, not_initialized),
    c(200, 0, not_initialized),
    c(300, 0, not_initialized),
    c(400, 8, initialize_ldn),
    c(401, 0, not_initialized),
    c(402, 16, initialize_ldn),
];
fn not_initialized(_: &mut Platform, _: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    Ok(Reply::result(NOT_INITIALIZED))
}
fn initialize_ldn(_: &mut Platform, r: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    if r.pid != Some(1) || (r.command_id == 402 && (r.u32(0)? as i32) < 0) {
        return Ok(Reply::result(0xc0cb));
    }
    // No radio backend: initialization cannot transition None -> Initialized.
    Ok(Reply::result(NOT_SUPPORTED))
}
pub(super) const LP2P_CREATOR: &[CommandSpec<Platform>] = &[c(0, 12, creator), c(8, 8, creator)];
fn creator(s: &mut Platform, r: &Request, x: &mut Context<'_>) -> Result<Reply, Error> {
    // CMIF orders the u32 before the aligned u64 placeholder.
    if r.pid != Some(1) || (r.command_id == 0 && r.u32(0)? != 1) {
        return Err(Error::Malformed("LP2P creator identity/options"));
    }
    s.child(
        if r.command_id == 0 {
            Kind::Lp2p
        } else {
            Kind::Lp2pMonitor
        },
        x,
    )
}
pub(super) const LP2P: &[CommandSpec<Platform>] = &[
    c(0, 0, initialize_lp2p),
    c(512, 0, unavailable),
    c(768, 0, unavailable),
    c(776, 0, initialize_lp2p),
    c(784, 0, unavailable),
    c(1536, 0, unavailable),
    c(1544, 0, unavailable),
];
pub(super) const MONITOR: &[CommandSpec<Platform>] = &[
    c(0, 0, initialize_lp2p),
    c(256, 0, event),
    c(264, 0, unavailable),
    c(272, 0, |_, _, _| Ok(Reply::data(vec![0]))),
    c(280, 0, unavailable),
    c(288, 0, unavailable),
    c(296, 0, unavailable),
    c(304, 0, unavailable),
    c(312, 0, unavailable),
    c(320, 0, |_, _, _| {
        Ok(Reply::data(0u32.to_le_bytes().to_vec()))
    }),
    c(328, 0, event),
    c(336, 0, unavailable),
];
fn initialize_lp2p(_: &mut Platform, _: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    // Documented Initialize is empty; DestroyGroup with role != 1 is also empty.
    Ok(Reply::success())
}
fn unavailable(_: &mut Platform, _: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    Ok(Reply::result(NOT_SUPPORTED))
}
fn event(s: &mut Platform, r: &Request, x: &mut Context<'_>) -> Result<Reply, Error> {
    let index = usize::from(r.command_id == 328);
    while s.owned_events.get(&x.object).map_or(0, Vec::len) <= index {
        let h = x.kernel.register_event(false).map_err(Error::Kernel)?;
        s.owned_events.entry(x.object).or_default().push(h);
    }
    let handle = if let Some(handle) = s.owned_events.get(&x.object).and_then(|v| v.get(index)) {
        *handle
    } else {
        let h = x.kernel.register_event(false).map_err(Error::Kernel)?;
        s.owned_events.entry(x.object).or_default().push(h);
        h
    };
    Ok(Reply::copy(handle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::platform::tests::{req, Harness};
    #[test]
    fn unavailable_radio_never_creates_a_network_or_changes_output_buffers() {
        let mut h = Harness::new();
        let factory = h.open("ldn:u");
        let ldn = h.child(factory, 0);
        let mut init = req(402, vec![0; 16]);
        init.pid = Some(1);
        init.data[0] = 1;
        assert_eq!(h.call(ldn, init).unwrap().result, NOT_SUPPORTED);
        assert_eq!(h.call(ldn, req(0, vec![])).unwrap().data, [0; 4]);
        for cmd in [1, 2, 102, 200, 300, 401] {
            let reply = h.call(ldn, req(cmd, vec![])).unwrap();
            assert_eq!(reply.result, NOT_INITIALIZED);
            assert!(reply.writes.is_empty());
        }
        let factory = h.open("lp2p:app");
        let mut create = req(0, vec![0; 16]);
        create.pid = Some(1);
        create.data[0] = 1;
        let service = h.call(factory, create).unwrap().objects[0];
        let mut create = req(8, vec![0; 8]);
        create.pid = Some(1);
        let monitor = h.call(factory, create).unwrap().objects[0];
        assert_eq!(h.call(service, req(0, vec![])).unwrap().result, 0);
        assert_eq!(
            h.call(service, req(768, vec![])).unwrap().result,
            NOT_SUPPORTED
        );
        assert_eq!(h.call(monitor, req(272, vec![])).unwrap().data, [0]);
        assert_eq!(h.call(service, req(776, vec![])).unwrap().result, 0);
        let a = h.call(monitor, req(256, vec![])).unwrap().copy_handles[0];
        let b = h.call(monitor, req(256, vec![])).unwrap().copy_handles[0];
        assert_eq!(a, b);
    }
}
