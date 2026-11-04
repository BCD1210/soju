//! Deliberately offline BSD/DNS profile. No host sockets or resolver calls.
use super::*;
use tables::c;
const ENETDOWN: u32 = 115;
const EAI_AGAIN: u32 = 2;
pub(super) const BSD: &[CommandSpec<Platform>] = &[
    c(0, 0, success),
    c(1, 0, success),
    c(2, 12, socket),
    c(3, 12, socket),
];
pub(super) const SSL: &[CommandSpec<Platform>] = &[c(0, 16, unavailable), c(5, 4, interface)];
pub(super) const DNS: &[CommandSpec<Platform>] =
    &[c(6, 16, resolve), c(8, 8, one), c(9, 16, success)];
fn socket(_: &mut Platform, _: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    Ok(Reply::data(
        [u32::MAX.to_le_bytes(), ENETDOWN.to_le_bytes()].concat(),
    ))
}
fn resolve(_: &mut Platform, _: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    Ok(Reply::data(
        [
            ENETDOWN.to_le_bytes(),
            EAI_AGAIN.to_le_bytes(),
            0u32.to_le_bytes(),
        ]
        .concat(),
    ))
}
fn interface(_: &mut Platform, req: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    if !(1..=3).contains(&req.u32(0)?) {
        return Err(Error::Malformed("SSL interface version"));
    }
    Ok(Reply::success())
}
pub(super) fn unavailable(
    _: &mut Platform,
    _: &Request,
    _: &mut Context<'_>,
) -> Result<Reply, Error> {
    Ok(Reply::result(0xf601))
}
pub(super) fn random(_: &mut Platform, req: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    use std::io::Read;
    let out = req.output()?;
    if out.len > 1024 * 1024 {
        return Err(Error::Malformed("random output limit"));
    }
    let mut bytes = vec![0; out.len as usize];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Reply::success().write(out, bytes)
}

#[cfg(test)]
mod tests {
    use super::super::tests::{Harness, req};
    #[test]
    fn offline_network_returns_guest_errors_without_allocating_sockets() {
        let mut h = Harness::new();
        let bsd = h.open("bsd:u");
        for id in [2, 3] {
            let reply = h.call(bsd, req(id, vec![0; 12])).unwrap();
            assert_eq!(reply.result, 0);
            assert_eq!(
                reply.data,
                [u32::MAX.to_le_bytes(), 115u32.to_le_bytes()].concat()
            );
            assert!(reply.objects.is_empty());
        }
        let dns = h.open("sfdnsres");
        assert_eq!(
            h.call(dns, req(6, vec![0; 16])).unwrap().data,
            [115u32.to_le_bytes(), 2u32.to_le_bytes(), 0u32.to_le_bytes()].concat()
        );
        let ssl = h.open("ssl");
        assert_eq!(
            h.call(ssl, req(5, 3u32.to_le_bytes().to_vec()))
                .unwrap()
                .result,
            0
        );
        assert_ne!(h.call(ssl, req(0, vec![0; 16])).unwrap().result, 0);
    }
}
