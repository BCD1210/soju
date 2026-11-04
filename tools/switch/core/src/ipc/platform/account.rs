//! One local offline account, shared with the existing player selector.
use super::*;
pub(super) const COMMANDS: &[CommandSpec<Platform>] = &[
    tables::c(0, 0, one),
    tables::c(2, 0, users),
    tables::c(4, 0, |_, _, _| Ok(Reply::data(uid().to_vec()))),
    tables::c(5, 16, profile),
    tables::c(50, 8, one),
    tables::c(51, 1, |_, _, _| Ok(Reply::data(uid().to_vec()))),
    tables::c(100, 8, success),
    tables::c(140, 8, success),
];
pub(super) const PROFILE: &[CommandSpec<Platform>] = &[
    tables::c(0, 0, profile_data),
    tables::c(1, 0, |_, _, _| Ok(Reply::data(base()))),
];
const fn uid() -> [u8; 16] {
    let mut value = [0; 16];
    value[0] = 1;
    value
}
fn users(_: &mut Platform, req: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    let buffer = req.output()?;
    if buffer.len != 8 * 16 {
        return Err(Error::Malformed("account list size"));
    }
    let mut bytes = vec![0; 8 * 16];
    bytes[..16].copy_from_slice(&uid());
    Reply::success().write(buffer, bytes)
}
fn profile(s: &mut Platform, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
    if req.data[..16] != uid() {
        return Err(Error::Malformed("unknown local account"));
    }
    s.child(Kind::Profile, ctx)
}
fn base() -> Vec<u8> {
    let mut bytes = vec![0; 56];
    bytes[..16].copy_from_slice(&uid());
    bytes[24..28].copy_from_slice(b"Soju");
    bytes
}
fn profile_data(_: &mut Platform, req: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    let buffer = req.output()?;
    if buffer.len != 128 {
        return Err(Error::Malformed("account user data size"));
    }
    Reply::data(base()).write(buffer, vec![0; 128])
}
