use super::*;
use std::collections::BTreeSet;
#[path = "binder.rs"]
mod binder;

#[derive(Default)]
pub(super) struct Display {
    pub open: bool,
    pub layer: bool,
    pub buffers: BTreeMap<u32, frame::GraphicBuffer>,
    pub available: BTreeSet<u32>,
    pub dequeued: BTreeSet<u32>,
    pub last: Option<u32>,
    pub event: Option<Handle>,
    pub vsync: Option<Handle>,
    pub next_vsync: u64,
}
impl Display {
    pub fn events(self) -> impl Iterator<Item = Handle> {
        self.event.into_iter().chain(self.vsync)
    }
    pub fn poll(&mut self, kernel: &mut Kernel, now: u64) -> Result<(), Error> {
        if let Some(event) = self.vsync
            && now >= self.next_vsync
        {
            kernel.signal_event(event).map_err(Error::Kernel)?;
            self.next_vsync = now.saturating_add(16_666_667);
        }
        Ok(())
    }
}
const fn command(
    id: u32,
    min: usize,
    handler: fn(&mut Media, &Request, &mut Context<'_>) -> Result<Reply, Error>,
) -> CommandSpec<Media> {
    CommandSpec {
        id,
        schema: Schema::at_least(min),
        handler,
    }
}
pub(super) const VI: &[CommandSpec<Media>] = &[command(2, 4, open_display_service)];
pub(super) const DISPLAY: &[CommandSpec<Media>] = &[
    command(100, 0, child),
    command(101, 0, child),
    command(102, 0, child),
    command(103, 0, child),
    command(1010, 64, open_display),
    command(1020, 8, close_display),
    command(2020, 80, open_layer),
    command(2021, 8, close_layer),
    command(2031, 8, close_layer),
    command(2101, 16, scale),
    command(5202, 8, vsync),
];
pub(super) const BINDER: &[CommandSpec<Media>] = &[
    command(1, 12, adjust),
    command(2, 8, event),
    command(3, 12, binder::transaction),
];
pub(super) fn state<'a>(media: &'a mut Media, ctx: &Context<'_>) -> Result<&'a mut Display, Error> {
    let owner = match media.objects.get(&ctx.object) {
        Some(Interface::Display(id) | Interface::Binder(id)) => *id,
        _ => return Err(Error::Malformed("display interface")),
    };
    media
        .displays
        .get_mut(&owner)
        .ok_or(Error::Malformed("display owner"))
}
fn open_display_service(
    media: &mut Media,
    _: &Request,
    ctx: &mut Context<'_>,
) -> Result<Reply, Error> {
    let (handle, id) = ctx.new_session(Family::Media)?;
    media.objects.insert(id, Interface::Display(id));
    media.displays.insert(id, Display::default());
    Ok(Reply::object(handle))
}
fn child(media: &mut Media, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
    let owner = match media.objects.get(&ctx.object) {
        Some(Interface::Display(id)) => *id,
        _ => return Err(Error::Malformed("display parent")),
    };
    let interface = if req.command_id == 100 {
        Interface::Binder(owner)
    } else {
        Interface::Child
    };
    Ok(Reply::object(media.session(interface, ctx)?))
}
fn open_display(media: &mut Media, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
    let name = req.data[..64].split(|b| *b == 0).next().unwrap_or_default();
    if name != b"Default" {
        return Err(Error::Malformed("display name"));
    }
    state(media, ctx)?.open = true;
    Ok(Reply::data(1u64.to_le_bytes().to_vec()))
}
fn close_display(media: &mut Media, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
    let display = state(media, ctx)?;
    if req.u64(0)? != 1 || !display.open || display.layer {
        return Err(Error::Malformed("display close"));
    }
    display.open = false;
    Ok(Reply::success())
}
fn open_layer(media: &mut Media, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
    let buffer = output_buffer(req, ctx, 28)?;
    let display = state(media, ctx)?;
    if !display.open {
        return Err(Error::Malformed("display not open"));
    }
    let reply = Reply::data(28u64.to_le_bytes().to_vec())
        .write(buffer, words(&[12, 16, 0, 28, 0, 0, 1]))?;
    display.layer = true;
    Ok(reply)
}
fn close_layer(media: &mut Media, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
    let display = state(media, ctx)?;
    if req.u64(0)? != 1 || !display.layer {
        return Err(Error::Malformed("layer close"));
    }
    display.layer = false;
    Ok(Reply::success())
}
fn scale(_: &mut Media, req: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    if req.u64(8)? != 1 || req.u32(0)? > 4 {
        return Err(Error::Malformed("layer scaling"));
    }
    Ok(Reply::success())
}
fn adjust(_: &mut Media, _: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    Ok(Reply::success())
}
fn event(media: &mut Media, _: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
    let display = state(media, ctx)?;
    let event = match display.event {
        Some(handle) => handle,
        None => {
            let handle = ctx
                .kernel
                .register_event(!display.available.is_empty() || display.buffers.is_empty())
                .map_err(Error::Kernel)?;
            display.event = Some(handle);
            handle
        }
    };
    Ok(Reply::copy(event))
}
fn vsync(media: &mut Media, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
    if req.u64(0)? != 1 {
        return Err(Error::Malformed("vsync display"));
    }
    let display = state(media, ctx)?;
    let event = match display.vsync {
        Some(handle) => handle,
        None => {
            let handle = ctx.kernel.register_event(false).map_err(Error::Kernel)?;
            display.vsync = Some(handle);
            display.next_vsync = ctx.now_ns.saturating_add(16_666_667);
            handle
        }
    };
    Ok(Reply::copy(event))
}
