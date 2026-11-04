use super::*;
use std::collections::VecDeque;
#[path = "pcm.rs"]
mod pcm;
use pcm::read_pcm;
const MAX_PCM: usize = 8 * 1024 * 1024;
struct Pending {
    tag: u64,
    remaining: u64,
    deadline: Option<u64>,
    pcm: Option<Vec<u8>>,
    bytes: usize,
}
pub(super) struct Output {
    pub running: bool,
    pub event: Handle,
    pending: VecDeque<Pending>,
    released: VecDeque<u64>,
    bytes: usize,
    completed_pcm: VecDeque<Vec<u8>>,
    completed_bytes: usize,
}
impl Output {
    fn new(event: Handle) -> Self {
        Self {
            running: false,
            event,
            pending: VecDeque::new(),
            released: VecDeque::new(),
            bytes: 0,
            completed_pcm: VecDeque::new(),
            completed_bytes: 0,
        }
    }
    pub fn poll(&mut self, kernel: &mut Kernel, now: u64) -> Result<(), Error> {
        while self.running
            && self
                .pending
                .front()
                .is_some_and(|p| p.deadline.is_some_and(|deadline| deadline <= now))
        {
            if let Some(item) = self.pending.pop_front() {
                self.bytes -= item.bytes;
                if let Some(pcm) = item.pcm {
                    // A detached host must not stall the guest's silent timing model.
                    while self.completed_bytes + pcm.len() > MAX_PCM {
                        if let Some(oldest) = self.completed_pcm.pop_front() {
                            self.completed_bytes -= oldest.len();
                        } else {
                            break;
                        }
                    }
                    self.completed_bytes += pcm.len();
                    self.completed_pcm.push_back(pcm);
                }
                self.released.push_back(item.tag);
                kernel.signal_event(self.event).map_err(Error::Kernel)?;
            }
        }
        Ok(())
    }
    fn set_running(&mut self, running: bool, now: u64) {
        if self.running == running {
            return;
        }
        let mut previous = now;
        for item in &mut self.pending {
            if running {
                previous = previous.saturating_add(item.remaining);
                item.deadline = Some(previous);
            } else if let Some(deadline) = item.deadline {
                item.remaining = deadline.saturating_sub(previous);
                item.deadline = None;
                previous = previous.max(deadline);
            }
        }
        self.running = running;
    }
    pub fn take_pcm(&mut self) -> Option<Vec<u8>> {
        if !self.running {
            return None;
        }
        if let Some(pcm) = self.completed_pcm.pop_front() {
            self.completed_bytes -= pcm.len();
            return Some(pcm);
        }
        self.pending.iter_mut().find_map(|item| item.pcm.take())
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
pub(super) const SERVICE: &[CommandSpec<Media>] = &[command(1, 8, open)];
pub(super) const OUTPUT: &[CommandSpec<Media>] = &[
    command(0, 0, status),
    command(1, 0, start),
    command(2, 0, stop),
    command(3, 8, append),
    command(4, 0, event),
    command(5, 0, released),
    command(6, 8, contains),
];
fn output<'a>(media: &'a mut Media, ctx: &mut Context<'_>) -> Result<&'a mut Output, Error> {
    let output = media
        .audio
        .get_mut(&ctx.object)
        .ok_or(Error::Malformed("audio object"))?;
    output.poll(ctx.kernel, ctx.now_ns)?;
    Ok(output)
}
fn open(media: &mut Media, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
    let input = req.input()?;
    if input.len < 256 || req.u32(0)? != 48000 || !matches!(req.u32(4)?, 2 | 0x20000) {
        return Err(Error::Malformed("audio format or device buffer"));
    }
    let mut name = [0; 256];
    ctx.memory.read(input.address, &mut name)?;
    let name = name.split(|b| *b == 0).next().unwrap_or_default();
    if !name.is_empty() && name != b"DeviceOut" {
        return Err(Error::Malformed("audio device name"));
    }
    let target = output_buffer(req, ctx, 256)?;
    let mut name = vec![0; 256];
    name[..9].copy_from_slice(b"DeviceOut");
    let event = ctx.kernel.register_event(false).map_err(Error::Kernel)?;
    let handle = match media.session(Interface::Output, ctx) {
        Ok(handle) => handle,
        Err(error) => {
            ctx.kernel
                .close_handle(ctx.memory, event)
                .map_err(Error::Kernel)?;
            return Err(error);
        }
    };
    let id = ctx.kernel.object_id(handle).map_err(Error::Kernel)?;
    media.audio.insert(id, Output::new(event));
    let mut reply = Reply::object(handle);
    reply.data = words(&[48000, 2, 2, 1]);
    reply.write(target, name)
}
fn status(media: &mut Media, _: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
    Ok(Reply::data(words(&[u32::from(
        !output(media, ctx)?.running,
    )])))
}
fn start(media: &mut Media, _: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
    output(media, ctx)?.set_running(true, ctx.now_ns);
    Ok(Reply::success())
}
fn stop(media: &mut Media, _: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
    output(media, ctx)?.set_running(false, ctx.now_ns);
    Ok(Reply::success())
}
fn append(media: &mut Media, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
    let pcm = read_pcm(req, ctx)?;
    let tag = req.u64(0)?;
    let now = ctx.now_ns;
    let output = output(media, ctx)?;
    if output.pending.len() + output.released.len() >= 64 || output.bytes + pcm.len() > MAX_PCM {
        return Err(Error::Malformed("audio queue full"));
    }
    let duration = u64::try_from(pcm.len()).map_err(|_| Error::Malformed("PCM length"))?
        * 1_000_000_000
        / 192000;
    let deadline = output.running.then(|| {
        output
            .pending
            .back()
            .and_then(|item| item.deadline)
            .unwrap_or(now)
            .max(now)
            .saturating_add(duration)
    });
    output.bytes += pcm.len();
    output.pending.push_back(Pending {
        tag,
        remaining: duration,
        deadline,
        bytes: pcm.len(),
        pcm: Some(pcm),
    });
    Ok(Reply::success())
}
fn event(media: &mut Media, _: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
    Ok(Reply::copy(output(media, ctx)?.event))
}
fn released(media: &mut Media, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
    let buffer = req.output()?;
    if buffer.len > MAX_PCM as u64 {
        return Err(Error::Malformed("audio release buffer limit"));
    }
    let output = output(media, ctx)?;
    let count = output
        .released
        .len()
        .min(usize::try_from(buffer.len / 8).map_err(|_| Error::Malformed("release count"))?);
    let target = output_buffer(req, ctx, count * 8)?;
    let bytes = output
        .released
        .iter()
        .take(count)
        .flat_map(|tag| tag.to_le_bytes())
        .collect();
    let reply = Reply::data(words(&[
        u32::try_from(count).map_err(|_| Error::Malformed("release count"))?
    ]))
    .write(target, bytes)?;
    output.released.drain(..count);
    Ok(reply)
}
fn contains(media: &mut Media, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
    let tag = req.u64(0)?;
    Ok(Reply::data(vec![u8::from(
        output(media, ctx)?
            .pending
            .iter()
            .any(|item| item.tag == tag),
    )]))
}
