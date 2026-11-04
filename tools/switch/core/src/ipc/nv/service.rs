//! NV service command schemas and resource lifetime.
use super::*;
pub(super) const COMMANDS: &[CommandSpec<Nv>] = &[
    CommandSpec {
        id: 0,
        schema: Schema::at_least(0),
        handler: open_device,
    },
    CommandSpec {
        id: 1,
        schema: Schema::at_least(8),
        handler: ioctl::dispatch,
    },
    CommandSpec {
        id: 2,
        schema: Schema::at_least(4),
        handler: close_device,
    },
    CommandSpec {
        id: 3,
        schema: Schema::at_least(4),
        handler: initialize,
    },
    CommandSpec {
        id: 4,
        schema: Schema::at_least(8),
        handler: event,
    },
    CommandSpec {
        id: 8,
        schema: Schema::at_least(0),
        handler: client,
    },
    CommandSpec {
        id: 11,
        schema: Schema::at_least(8),
        handler: super::submit::dispatch,
    },
];
fn open_device(nv: &mut Nv, r: &Request, c: &mut Context<'_>) -> Result<Reply, Error> {
    if r.count(BufferKind::Send) != 1 {
        return Err(Error::Malformed("NV open descriptors"));
    }
    let bytes = c.read_buffer(r.buffer(BufferKind::Send, 0)?, 64)?;
    let name = bytes
        .split(|&b| b == 0)
        .next()
        .ok_or(Error::Malformed("NV device name"))?;
    let device = match name {
        b"/dev/nvmap" => Device::Nvmap,
        b"/dev/nvhost-ctrl" => Device::Ctrl,
        b"/dev/nvhost-ctrl-gpu" => Device::CtrlGpu,
        b"/dev/nvhost-gpu" => Device::Gpu,
        b"/dev/nvhost-as-gpu" => Device::AsGpu,
        _ => return Err(Error::Malformed("unsupported NV device")),
    };
    if device == Device::Gpu && nv.channels.len() + nv.retired_syncpoints.len() >= 4096 {
        return Err(Error::Malformed("NV syncpoint capacity exhausted"));
    }
    let fd = nv
        .next_fd
        .checked_add(1)
        .ok_or(Error::Malformed("NV fd exhausted"))?;
    nv.next_fd = fd;
    nv.files.insert(
        fd,
        File {
            owner: c.object,
            device,
        },
    );
    if device == Device::Gpu {
        nv.channels.insert(
            fd,
            Channel {
                priority: 100,
                ..Channel::default()
            },
        );
    }
    let mut bytes = vec![0; 8];
    put32(&mut bytes, 0, fd);
    Ok(Reply::data(bytes))
}
fn close_device(nv: &mut Nv, r: &Request, c: &mut Context<'_>) -> Result<Reply, Error> {
    let fd = r.u32(0)?;
    nv.file(fd, c.object)?;
    nv.remove_fd(fd);
    Ok(Reply::data(vec![0; 4]))
}
fn initialize(nv: &mut Nv, r: &Request, c: &mut Context<'_>) -> Result<Reply, Error> {
    if r.copy_handles.len() != 2
        || !r.move_handles.is_empty()
        || r.copy_handles[0] != 0xffff8001
        || nv.transfers.contains_key(&c.object)
    {
        return Err(Error::Malformed("NV initialization handles"));
    }
    let handle = r.copy_handles[1];
    let id = c.kernel.object_id(handle).map_err(Error::Kernel)?;
    let (_, size) = c
        .kernel
        .transfer(id)
        .ok_or(Error::Malformed("NV transfer memory"))?;
    if u64::from(r.u32(0)?)
        != u64::try_from(size).map_err(|_| Error::Malformed("NV transfer size"))?
        || nv.transfers.values().any(|&v| v == id)
    {
        return Err(Error::Malformed("NV transfer ownership or size"));
    }
    let retained = c.kernel.retain_object(handle).map_err(Error::Kernel)?;
    nv.transfers.insert(c.object, retained);
    Ok(Reply::data(vec![0; 4]))
}
fn event(nv: &mut Nv, r: &Request, c: &mut Context<'_>) -> Result<Reply, Error> {
    let fd = r.u32(0)?;
    if nv.file(fd, c.object)? == Device::Ctrl {
        let id = r.u32(4)?;
        if id & !63 != 0x10000000 {
            return Err(Error::Malformed("NV control event ID"));
        }
        let handle = *nv
            .fence_events
            .get(&(fd, id & 63))
            .ok_or(Error::Malformed("NV event not registered"))?;
        let mut reply = Reply::copy(handle);
        reply.data = vec![0; 4];
        return Ok(reply);
    }
    if nv.file(fd, c.object)? != Device::Gpu || r.u32(4)? != 3 {
        return Err(Error::Malformed("NV channel event"));
    }
    let channel = nv
        .channels
        .get_mut(&fd)
        .ok_or(Error::Malformed("NV channel"))?;
    let event = match channel.event {
        Some(h) => h,
        None => {
            let h = c.kernel.register_event(false).map_err(Error::Kernel)?;
            channel.event = Some(h);
            h
        }
    };
    let mut reply = Reply::copy(event);
    reply.data = vec![0; 4];
    Ok(reply)
}
fn client(_: &mut Nv, _: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    Ok(Reply::data(vec![0; 4]))
}
