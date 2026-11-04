//! Public libnx control-event ABI for already completed synchronous submissions.
use super::*;
pub(super) fn register(nv: &mut Nv, r: &Request, c: &mut Context<'_>) -> Result<Reply, Error> {
    let fd = r.u32(0)?;
    let index = r.u32(4)?;
    if index >= 64 || nv.fence_events.contains_key(&(fd, index)) {
        return Err(Error::Malformed("NV event slot unavailable"));
    }
    let event = c.kernel.register_event(false).map_err(Error::Kernel)?;
    nv.fence_events.insert((fd, index), event);
    Ok(Reply::data(ioctl::args(r).to_vec()))
}
pub(super) fn unregister(nv: &mut Nv, r: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    let event = nv
        .fence_events
        .remove(&(r.u32(0)?, r.u32(4)?))
        .ok_or(Error::Malformed("NV event not registered"))?;
    nv.close.push(event);
    Ok(Reply::data(ioctl::args(r).to_vec()))
}
pub(super) fn read(nv: &mut Nv, r: &Request, c: &mut Context<'_>) -> Result<Reply, Error> {
    let syncpoint = r.u32(4)?;
    let (completed, _) = nv.syncpoint(syncpoint, c.object)?;
    let mut data = ioctl::args(r).to_vec();
    put32(&mut data, 4, completed);
    Ok(Reply::data(data))
}
pub(super) fn wait(nv: &mut Nv, r: &Request, c: &mut Context<'_>) -> Result<Reply, Error> {
    let fd = r.u32(0)?;
    let syncpoint = r.u32(4)?;
    let threshold = r.u32(8)?;
    let index = r.u32(16)?;
    if !nv.fence_events.contains_key(&(fd, index)) {
        return Err(Error::Malformed("NV event not registered"));
    }
    let (completed, faulted) = nv.syncpoint(syncpoint, c.object)?;
    if faulted || completed < threshold {
        return Err(Error::Malformed("NV fence has not completed"));
    }
    Ok(Reply::data(ioctl::args(r).to_vec()))
}
