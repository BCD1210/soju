//! NVMap allocation identity and session/process ownership.
use super::*;
use ioctl::args;
pub(super) fn create(nv: &mut Nv, r: &Request, c: &mut Context<'_>) -> Result<Reply, Error> {
    let mut data = args(r).to_vec();
    let size = read_u32(&data, 0)?;
    if size == 0 || size > 1 << 30 {
        return Err(Error::Malformed("NV allocation size"));
    }
    let id = (1..=4096)
        .find(|id| !nv.allocations.contains_key(id))
        .ok_or(Error::Malformed("NV allocation limit"))?;
    nv.generation = nv
        .generation
        .checked_add(1)
        .ok_or(Error::Malformed("NV generation exhausted"))?;
    nv.allocations.insert(
        id,
        Allocation {
            owner: c.object,
            client: c.client,
            flags: 0,
            view: NvBufferView {
                allocation: id,
                generation: nv.generation,
                guest_address: 0,
                size: u64::from(size),
                alignment: 4096,
                kind: 0,
            },
        },
    );
    put32(&mut data, 4, id);
    Ok(Reply::data(data))
}
pub(super) fn allocate(nv: &mut Nv, r: &Request, c: &mut Context<'_>) -> Result<Reply, Error> {
    let data = args(r);
    let id = read_u32(data, 0)?;
    let address = read_u64(data, 24)?;
    let a = nv.allocation(id, c.object)?;
    let size = usize::try_from(a.view.size).map_err(|_| Error::Malformed("NV allocation size"))?;
    if a.view.guest_address != 0
        || address % 4096 != 0
        || address < crate::kernel::HEAP
        || address.checked_add(a.view.size).is_none_or(|end| {
            end > crate::kernel::HEAP + u64::try_from(crate::kernel::HEAP_LIMIT).unwrap_or(0)
        })
    {
        return Err(Error::Malformed("NV allocation address"));
    }
    c.memory.check_access(address, size, 1)?;
    let mut updated = a.clone();
    updated.view.guest_address = address;
    updated.flags = read_u32(data, 8)?;
    updated.view.alignment = u64::from(read_u32(data, 12)?).max(4096);
    updated.view.kind = u32::from(data[16]);
    nv.allocations.insert(id, updated);
    Ok(Reply::data(data.to_vec()))
}
pub(super) fn free(nv: &mut Nv, r: &Request, c: &mut Context<'_>) -> Result<Reply, Error> {
    let mut data = args(r).to_vec();
    let id = read_u32(&data, 0)?;
    let a = nv.allocation(id, c.object)?;
    if nv
        .spaces
        .values()
        .any(|s| s.mappings.values().any(|m| m.view.allocation == id))
    {
        return Err(Error::Malformed("NV allocation remains GPU mapped"));
    }
    put64(&mut data, 8, a.view.guest_address);
    put32(
        &mut data,
        16,
        u32::try_from(a.view.size).map_err(|_| Error::Malformed("NV allocation size"))?,
    );
    put32(&mut data, 20, u32::from(a.flags & 2 != 0));
    nv.allocations.remove(&id);
    Ok(Reply::data(data))
}
pub(super) fn get_id(nv: &mut Nv, r: &Request, c: &mut Context<'_>) -> Result<Reply, Error> {
    let mut data = args(r).to_vec();
    let id = read_u32(&data, 4)?;
    nv.allocation(id, c.object)?;
    put32(&mut data, 0, id);
    Ok(Reply::data(data))
}
