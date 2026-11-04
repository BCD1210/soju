//! Typed per-device ioctl dispatch; arguments are normalized before handlers.
use super::*;
const fn spec(
    id: u32,
    size: usize,
    handler: fn(&mut Nv, &Request, &mut Context<'_>) -> Result<Reply, Error>,
) -> CommandSpec<Nv> {
    CommandSpec {
        id,
        schema: Schema {
            min_data: size + 4,
            max_data: size + 4,
        },
        handler,
    }
}
const NVMAP: &[CommandSpec<Nv>] = &[
    spec(0xc0080101, 8, nvmap::create),
    spec(0xc0200104, 32, nvmap::allocate),
    spec(0xc0180105, 24, nvmap::free),
    spec(0xc008010e, 8, nvmap::get_id),
];
const SYNC: &[CommandSpec<Nv>] = &[
    spec(0xc004001f, 4, fences::register),
    spec(0xc0040020, 4, fences::unregister),
    spec(0xc0080014, 8, fences::read),
    spec(0xc010001e, 16, fences::wait),
];
const CTRL: &[CommandSpec<Nv>] = &[
    spec(0xc0184706, 24, queries::tpc_masks),
    spec(0xc0b04705, 176, control::characteristics),
    spec(0x80044701, 0, control::zcull_size),
    spec(0x80284702, 0, control::zcull_info),
];
const AS: &[CommandSpec<Nv>] = &[
    spec(0x40284109, 40, control::initialize_space),
    spec(0x40044101, 4, control::bind_space),
    spec(0xc0184102, 24, control::reserve),
    spec(0xc0284106, 40, control::map),
];
const GPU: &[CommandSpec<Nv>] = &[
    spec(0x40044801, 4, control::bind_nvmap),
    spec(0xc020481a, 32, control::gpfifo),
    spec(0xc0104809, 16, control::object),
    spec(0xc018480c, 24, control::notifier),
    spec(0x4004480d, 4, control::priority),
    spec(0xc010480b, 16, control::zcull),
];
pub(super) fn dispatch(nv: &mut Nv, r: &Request, c: &mut Context<'_>) -> Result<Reply, Error> {
    let _span = soju_switch_gpu::profile::span(soju_switch_gpu::profile::Stage::NvIoctl);
    if (
        r.count(BufferKind::Pointer),
        r.count(BufferKind::Send),
        r.count(BufferKind::Receive),
    ) != (1, 1, 1)
    {
        return Err(Error::Malformed("NV ioctl descriptors"));
    }
    let fd = r.u32(0)?;
    let command = r.u32(4)?;
    let table = match nv.file(fd, c.object)? {
        Device::Nvmap => NVMAP,
        Device::Ctrl => SYNC,
        Device::CtrlGpu => CTRL,
        Device::AsGpu => AS,
        Device::Gpu => GPU,
    };
    let input = r.buffer(BufferKind::Send, 0)?;
    let output = r.buffer(BufferKind::Receive, 0)?;
    let mut data = fd.to_le_bytes().to_vec();
    data.extend(c.read_buffer(input, 176)?);
    let output_size = match command {
        0x40284109 | 0x40044101 | 0x40044801 | 0x4004480d => 0,
        0x80044701 => 4,
        0x80284702 => 40,
        _ => input.len,
    };
    let exact = matches!(
        command,
        0x40284109
            | 0x40044101
            | 0x40044801
            | 0x4004480d
            | 0x80044701
            | 0x80284702
            | 0xc020481a
            | 0xc0104809
            | 0xc018480c
            | 0xc0284106
            | 0xc010480b
            | 0xc0184102
    );
    if output.len > u64::try_from(MAX_BUFFER).map_err(|_| Error::Malformed("NV output limit"))?
        || output.len < output_size
        || (exact && output.len != output_size)
    {
        return Err(Error::Malformed("NV ioctl output size"));
    }
    c.memory.check_access(
        output.address,
        usize::try_from(output.len).map_err(|_| Error::Malformed("NV output bounds"))?,
        2,
    )?;
    let mut inner = r.clone();
    inner.command_id = command;
    inner.data = data;
    let bytes = dispatch_table(nv, table, &inner, c)
        .map_err(|error| Error::Message(format!("NV ioctl {command:#010x} fd={fd}: {error}")))?
        .data;
    Reply::data(vec![0; 4]).write(output, bytes)
}
pub(super) fn args(r: &Request) -> &[u8] {
    &r.data[4..]
}
