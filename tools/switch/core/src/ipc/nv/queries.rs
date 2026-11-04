//! GPU topology queries used by libnx and nouveau initialization.
use super::*;
pub(super) fn tpc_masks(_: &mut Nv, r: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    let mut data = ioctl::args(r).to_vec();
    if read_u32(&data, 0)? != 4 || read_u32(&data, 4)? != 0 || read_u64(&data, 8)? == 0 {
        return Err(Error::Malformed("GPU TPC mask buffer"));
    }
    // One GPC with two TPCs, consistent with the characteristics reply.
    put32(&mut data, 16, 3);
    put32(&mut data, 20, 0);
    Ok(Reply::data(data))
}
