//! Virtual GPU query replies and control ioctl state.
use super::*;
use ioctl::args;
pub(super) fn characteristics(
    _: &mut Nv,
    r: &Request,
    _: &mut Context<'_>,
) -> Result<Reply, Error> {
    let mut data = args(r).to_vec();
    if read_u64(&data, 0)? == 0 || read_u64(&data, 8)? == 0 {
        return Err(Error::Malformed("GPU characteristics request"));
    }
    put64(&mut data, 0, 0xa0);
    for (i, v) in [0x120, 0xb, 0xa1, 1].iter().enumerate() {
        put32(&mut data, 0x10 + i * 4, *v);
    }
    put64(&mut data, 0x20, 0x40000);
    put64(&mut data, 0x28, 0);
    for (i, v) in [
        2, 0x20, 0x20000, 0x20000, 0x1b, 0x30000, 1, 0x503, 0x503, 0x80, 0x28, 0,
    ]
    .iter()
    .enumerate()
    {
        put32(&mut data, 0x30 + i * 4, *v);
    }
    put64(&mut data, 0x60, 0x55);
    for (i, v) in [
        0x902d, 0xb197, 0xb1c0, 0xb06f, 0xa140, 0xb0b5, 1, 0, 2, 1, 0, 1, 0x21d70, 0,
    ]
    .iter()
    .enumerate()
    {
        put32(&mut data, 0x68 + i * 4, *v);
    }
    put64(&mut data, 0xa0, 0x6230326d67);
    put64(&mut data, 0xa8, 0);
    Ok(Reply::data(data))
}
pub(super) fn zcull_size(_: &mut Nv, _: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    Ok(Reply::data(0x1000u32.to_le_bytes().to_vec()))
}
pub(super) fn zcull_info(_: &mut Nv, _: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    Ok(Reply::data(
        [
            0x20u32, 0x20, 0x400, 0x800, 0x20, 0x20, 0xc0, 0x20, 0x40, 0x10,
        ]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect(),
    ))
}
pub(super) fn initialize_space(
    nv: &mut Nv,
    r: &Request,
    _: &mut Context<'_>,
) -> Result<Reply, Error> {
    let fd = r.u32(0)?;
    let d = args(r);
    if nv.spaces.contains_key(&fd)
        || read_u32(d, 0)? != 1
        || read_u32(d, 4)? != 0
        || !matches!(read_u32(d, 8)?, 65536 | 131072)
        || d[12..].iter().any(|&b| b != 0)
    {
        return Err(Error::Message(format!(
            "GPU address space initialization: {:02x?}",
            d
        )));
    }
    nv.spaces.insert(fd, memory::AddressSpace::default());
    Ok(Reply::success())
}
pub(super) fn bind_space(nv: &mut Nv, r: &Request, c: &mut Context<'_>) -> Result<Reply, Error> {
    let fd = r.u32(0)?;
    let channel_fd = read_u32(args(r), 0)?;
    if !nv.spaces.contains_key(&fd) || nv.file(channel_fd, c.object)? != Device::Gpu {
        return Err(Error::Malformed("GPU address space binding"));
    }
    let channel = nv
        .channels
        .get_mut(&channel_fd)
        .ok_or(Error::Malformed("GPU channel"))?;
    if channel.space.is_some() {
        return Err(Error::Malformed("duplicate GPU address space binding"));
    }
    channel.space = Some(fd);
    Ok(Reply::success())
}
pub(super) fn reserve(nv: &mut Nv, r: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    let mut d = args(r).to_vec();
    let space = nv
        .spaces
        .get_mut(&r.u32(0)?)
        .ok_or(Error::Malformed("uninitialized GPU address space"))?;
    let address = space.reserve(&d)?;
    put64(&mut d, 16, address);
    Ok(Reply::data(d))
}
pub(super) fn map(nv: &mut Nv, r: &Request, c: &mut Context<'_>) -> Result<Reply, Error> {
    let mut d = args(r).to_vec();
    let a = nv.allocation(read_u32(&d, 8)?, c.object)?.view;
    c.memory.check_access(
        a.guest_address,
        usize::try_from(a.size).map_err(|_| Error::Malformed("GPU backing size"))?,
        1,
    )?;
    let space = nv
        .spaces
        .get_mut(&r.u32(0)?)
        .ok_or(Error::Malformed("uninitialized GPU address space"))?;
    let address = space.map(a, &d)?;
    put64(&mut d, 32, address);
    Ok(Reply::data(d))
}
pub(super) fn bind_nvmap(nv: &mut Nv, r: &Request, c: &mut Context<'_>) -> Result<Reply, Error> {
    let binding = read_u32(args(r), 0)?;
    if nv.file(binding, c.object)? != Device::Nvmap {
        return Err(Error::Malformed("GPU NVMap binding"));
    }
    let channel = channel(nv, r)?;
    if channel.nvmap.is_some() {
        return Err(Error::Malformed("duplicate GPU NVMap binding"));
    }
    channel.nvmap = Some(binding);
    Ok(Reply::success())
}
pub(super) fn gpfifo(nv: &mut Nv, r: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    let mut d = args(r).to_vec();
    let entries = read_u32(&d, 0)?;
    let channel = channel(nv, r)?;
    if channel.space.is_none()
        || channel.nvmap.is_none()
        || channel.capacity.is_some()
        || entries == 0
        || entries > 65536
        || !entries.is_power_of_two()
        || read_u32(&d, 4)? != 1
        || d[8..].iter().any(|&b| b != 0)
    {
        return Err(Error::Malformed("GPU GPFIFO allocation"));
    }
    channel.capacity = Some(entries);
    put32(&mut d, 12, r.u32(0)?);
    put32(&mut d, 16, 0);
    Ok(Reply::data(d))
}
pub(super) fn object(nv: &mut Nv, r: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    let mut d = args(r).to_vec();
    let channel = channel(nv, r)?;
    if channel.capacity.is_none()
        || read_u32(&d, 0)? != 0xb197
        || read_u32(&d, 4)? != 0
        || channel.objects >= 16
    {
        return Err(Error::Malformed("GPU object context"));
    }
    channel.objects += 1;
    put64(&mut d, 8, channel.objects);
    Ok(Reply::data(d))
}
pub(super) fn notifier(nv: &mut Nv, r: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    let d = args(r);
    let enabled = read_u32(d, 16)?;
    if read_u64(d, 0)? != 0 || read_u64(d, 8)? != 0 || enabled > 1 || read_u32(d, 20)? != 0 {
        return Err(Error::Malformed("GPU error notifier"));
    }
    channel(nv, r)?.notifier = enabled == 1;
    Ok(Reply::data(d.to_vec()))
}
pub(super) fn priority(nv: &mut Nv, r: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    let priority = read_u32(args(r), 0)?;
    if !matches!(priority, 50 | 100 | 150) {
        return Err(Error::Malformed("GPU priority"));
    }
    channel(nv, r)?.priority = priority;
    Ok(Reply::success())
}
pub(super) fn zcull(nv: &mut Nv, r: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    let d = args(r);
    let address = read_u64(d, 0)?;
    let ch = channel(nv, r)?;
    let space_id = ch
        .space
        .ok_or(Error::Malformed("GPU ZCull address space"))?;
    if ch.capacity.is_none() || read_u32(d, 8)? != 2 || read_u32(d, 12)? != 0 || address % 4096 != 0
    {
        return Err(Error::Malformed("GPU ZCull binding"));
    }
    nv.spaces
        .get(&space_id)
        .ok_or(Error::Malformed("GPU address space"))?
        .resolve(address, 4096)?;
    channel(nv, r)?.zcull = Some(address);
    Ok(Reply::data(d.to_vec()))
}
fn channel<'a>(nv: &'a mut Nv, r: &Request) -> Result<&'a mut Channel, Error> {
    nv.channels
        .get_mut(&r.u32(0)?)
        .ok_or(Error::Malformed("GPU channel"))
}
