use super::*;

pub(super) fn transaction(
    media: &mut Media,
    req: &Request,
    ctx: &mut Context<'_>,
) -> Result<Reply, Error> {
    let mut transaction = req.clone();
    transaction.command_id = req.u32(4)?;
    dispatch_table(media, TRANSACTIONS, &transaction, ctx)
}
const TRANSACTIONS: &[CommandSpec<Media>] = &[
    command(1, 12, request),
    command(3, 12, dequeue),
    command(7, 12, queue),
    command(10, 12, connect),
    command(11, 12, disconnect),
    command(14, 12, preallocate),
];
fn parcel(payload: &[u8]) -> Vec<u8> {
    let length = u32::try_from(payload.len()).unwrap_or(0);
    let mut out = words(&[length, 16, 0, 16 + length]);
    out.extend_from_slice(payload);
    out
}
fn reply(req: &Request, ctx: &Context<'_>, payload: &[u8]) -> Result<Reply, Error> {
    let bytes = parcel(payload);
    Reply::success().write(output_buffer(req, ctx, bytes.len())?, bytes)
}
fn input(req: &Request, ctx: &Context<'_>) -> Result<(Vec<u8>, usize), Error> {
    let raw = ctx.read_buffer(req.input()?, 1024)?;
    if raw.len() < 32 || read_u32(&raw, 4)? != 16 {
        return Err(Error::Malformed("Binder parcel header"));
    }
    let size = usize::try_from(read_u32(&raw, 0)?).map_err(|_| Error::Malformed("parcel size"))?;
    if size > raw.len() - 16 || read_u32(&raw, 16)? != 0x100 || read_u32(&raw, 20)? > 128 {
        return Err(Error::Malformed("Binder parcel size or token"));
    }
    let length =
        usize::try_from(read_u32(&raw, 20)?).map_err(|_| Error::Malformed("token length"))?;
    let end = 24 + (length + 1) * 2;
    let position = (end + 3) & !3;
    let expected: Vec<u8> = "android.gui.IGraphicBufferProducer"
        .encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect();
    if position + 4 > 16 + size || raw.get(24..end) != Some(expected.as_slice()) {
        return Err(Error::Malformed("Binder interface token"));
    }
    Ok((raw[..16 + size].to_vec(), position))
}
fn argument(req: &Request, ctx: &Context<'_>) -> Result<u32, Error> {
    let (raw, position) = input(req, ctx)?;
    read_u32(&raw, position)
}
fn connect(_: &mut Media, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
    input(req, ctx)?;
    reply(req, ctx, &words(&[1280, 720, 0, 0, 0]))
}
fn disconnect(media: &mut Media, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
    if !(1..=4).contains(&argument(req, ctx)?) {
        return Err(Error::Malformed("Binder disconnect API"));
    }
    let reply = reply(req, ctx, &words(&[0]))?;
    let display = state(media, ctx)?;
    display.buffers.clear();
    display.available.clear();
    display.dequeued.clear();
    display.last = None;
    Ok(reply)
}
fn request(media: &mut Media, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
    let slot = argument(req, ctx)?;
    if !state(media, ctx)?.dequeued.contains(&slot) {
        return Err(Error::Malformed("request buffer ownership"));
    }
    reply(req, ctx, &words(&[0, 0]))
}
fn dequeue(media: &mut Media, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
    input(req, ctx)?;
    let display = state(media, ctx)?;
    let slot = display
        .available
        .iter()
        .find(|s| display.last.is_none_or(|last| **s > last))
        .or_else(|| display.available.first())
        .copied()
        .ok_or(Error::Malformed("no available display buffer"))?;
    let mut payload = words(&[slot, 1, 36, 0]);
    payload.extend_from_slice(&[0; 40]);
    let reply = reply(req, ctx, &payload)?;
    display.available.remove(&slot);
    display.dequeued.insert(slot);
    display.last = Some(slot);
    if display.available.is_empty()
        && let Some(event) = display.event
    {
        ctx.kernel.clear_event(event).map_err(Error::Kernel)?;
    }
    Ok(reply)
}
fn queue(media: &mut Media, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
    let slot = argument(req, ctx)?;
    let display = state(media, ctx)?;
    if !display.dequeued.contains(&slot) {
        return Err(Error::Malformed("queue buffer ownership"));
    }
    let buffer = display
        .buffers
        .get(&slot)
        .ok_or(Error::Malformed("buffer slot"))?;
    let reply = reply(req, ctx, &words(&[buffer.width, buffer.height, 0, 0, 0]))?;
    let started = std::time::Instant::now();
    let frame = buffer.pixels(ctx)?;
    let conversion_time = started.elapsed();
    display.dequeued.remove(&slot);
    display.available.insert(slot);
    if let Some(event) = display.event {
        ctx.kernel.signal_event(event).map_err(Error::Kernel)?;
    }
    media.frame_conversion_time += conversion_time;
    media.frame = Some(frame);
    Ok(reply)
}
fn preallocate(media: &mut Media, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
    let (raw, position) = input(req, ctx)?;
    let magic = raw
        .windows(4)
        .position(|v| v == b"RFBG")
        .ok_or(Error::Malformed("graphic buffer magic"))?;
    if magic != position + 16 || magic + 132 > raw.len() {
        return Err(Error::Malformed("flattened graphic buffer"));
    }
    let slot = read_u32(&raw, position)?;
    let id = read_u32(&raw, magic + 44)?;
    if ctx.client != ClientId(1) {
        return Err(Error::Malformed("display client"));
    }
    let allocation = ctx
        .nv
        .ok_or(Error::Malformed("missing NV allocator"))?
        .resolve(ClientId(1), id)?;
    if allocation.guest_address == 0 {
        return Err(Error::Malformed("unmapped display allocation"));
    }
    let buffer = frame::GraphicBuffer {
        id,
        allocation,
        width: read_u32(&raw, magic + 92)?,
        height: read_u32(&raw, magic + 96)?,
        pitch: read_u32(&raw, magic + 112)?,
        plane: read_u32(&raw, magic + 120)?,
        block: read_u32(&raw, magic + 128)?,
        format: if read_u32(&raw, magic + 16)? == 0 && read_u32(&raw, magic + 52)? == 0xdaffcaff {
            read_u32(&raw, magic + 68)?
        } else {
            read_u32(&raw, magic + 16)?
        },
    };
    buffer.size()?;
    buffer.fresh(ctx)?;
    let reply = reply(req, ctx, &[])?;
    let display = state(media, ctx)?;
    if slot >= 64 || display.dequeued.contains(&slot) {
        return Err(Error::Malformed("invalid or busy display slot"));
    }
    display.buffers.insert(slot, buffer);
    display.available.insert(slot);
    if let Some(event) = display.event {
        ctx.kernel.signal_event(event).map_err(Error::Kernel)?;
    }
    Ok(reply)
}
