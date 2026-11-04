//! Timed capture queues; completion events follow publication of PCM bytes.
use super::microphone::Microphone;
use super::*;
use std::collections::VecDeque;
const MAX_BYTES: usize = 8 * 1024 * 1024;
struct Pending {
    tag: u64,
    address: u64,
    bytes: usize,
    remaining: u64,
    deadline: Option<u64>,
}
pub(super) struct Input {
    pub(super) event: Handle,
    running: bool,
    native_enabled: bool,
    channels: usize,
    pending: VecDeque<Pending>,
    released: VecDeque<u64>,
    bytes: usize,
    gain: f32,
    mic: Microphone,
}
impl Input {
    fn running(&mut self, running: bool, now: u64) {
        if self.running == running {
            return;
        }
        self.mic = if running && self.native_enabled {
            Microphone::start()
        } else {
            Microphone::default()
        };
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
    pub(super) fn poll(
        &mut self,
        kernel: &mut Kernel,
        memory: &mut dyn GuestMemory,
        now: u64,
    ) -> Result<(), Error> {
        while self.running
            && self
                .pending
                .front()
                .is_some_and(|p| p.deadline.is_some_and(|d| d <= now))
        {
            let p = self.pending.front().unwrap();
            memory.check_access(p.address, p.bytes, 2)?;
            let frames = p.bytes / (self.channels * 2);
            let stereo = self.mic.read(frames * 2);
            let mut pcm = Vec::with_capacity(p.bytes);
            for frame in stereo.chunks_exact(2) {
                for channel in 0..self.channels {
                    let sample = if self.channels == 1 {
                        (i32::from(frame[0]) + i32::from(frame[1])) / 2
                    } else {
                        i32::from(frame[channel])
                    };
                    let sample = (sample as f32 * self.gain).round().clamp(-32768., 32767.) as i16;
                    pcm.extend(sample.to_le_bytes());
                }
            }
            memory.write(p.address, &pcm)?;
            let p = self.pending.pop_front().unwrap();
            self.bytes -= p.bytes;
            self.released.push_back(p.tag);
            kernel.signal_event(self.event).map_err(Error::Kernel)?;
        }
        Ok(())
    }
}
const fn c(
    id: u32,
    n: usize,
    handler: fn(&mut Media, &Request, &mut Context<'_>) -> Result<Reply, Error>,
) -> CommandSpec<Media> {
    CommandSpec {
        id,
        schema: Schema::at_least(n),
        handler,
    }
}
pub(super) const SERVICE: &[CommandSpec<Media>] = &[
    c(0, 0, list),
    c(1, 16, open),
    c(2, 0, list),
    c(3, 16, open),
    c(4, 0, list),
];
pub(super) const INPUT: &[CommandSpec<Media>] = &[
    c(0, 0, command),
    c(1, 0, command),
    c(2, 0, command),
    c(3, 8, append),
    c(4, 0, command),
    c(5, 0, released),
    c(6, 8, command),
    c(8, 8, append),
    c(9, 0, released),
    c(11, 0, command),
    c(12, 4, command),
    c(13, 0, command),
    c(14, 0, command),
];
fn list(_: &mut Media, r: &Request, x: &mut Context<'_>) -> Result<Reply, Error> {
    let out = r.output()?;
    if out.len > MAX_BYTES as u64 || !out.len.is_multiple_of(256) {
        return Err(Error::Malformed("audio input device array"));
    }
    x.memory.check_access(out.address, out.len as usize, 2)?;
    if out.len == 0 {
        return Ok(Reply::data(words(&[0])));
    }
    let mut name = vec![0; 256];
    name[..8].copy_from_slice(b"DeviceIn");
    Reply::data(words(&[1])).write(out, name)
}
fn open(m: &mut Media, r: &Request, x: &mut Context<'_>) -> Result<Reply, Error> {
    if r.pid.is_none() || r.copy_handles != [0xffff8001] || !r.move_handles.is_empty() {
        return Err(Error::Malformed("audio input process handle/PID"));
    }
    if !matches!(r.u32(0)?, 0 | 48000) || !matches!(r.u32(4)?, 0 | 1 | 2 | 0x10000 | 0x20000) {
        return Err(Error::Message(format!(
            "audio input format {} {:#x}",
            r.u32(0)?,
            r.u32(4)?
        )));
    }
    let channels = if matches!(r.u32(4)?, 1 | 0x10000) {
        1
    } else {
        2
    };
    let name = x.read_buffer(r.input()?, 256)?;
    let end = name
        .iter()
        .position(|b| *b == 0)
        .ok_or(Error::Malformed("audio input name termination"))?;
    if end > 0 && &name[..end] != b"DeviceIn" {
        return Err(Error::Malformed("unknown audio input device"));
    }
    let out = output_buffer(r, x, 256)?;
    let event = x.kernel.register_event(false).map_err(Error::Kernel)?;
    let handle = match m.session(Interface::Input, x) {
        Ok(h) => h,
        Err(e) => {
            x.kernel
                .close_handle(x.memory, event)
                .map_err(Error::Kernel)?;
            return Err(e);
        }
    };
    let id = x.kernel.object_id(handle).map_err(Error::Kernel)?;
    m.inputs.insert(
        id,
        Input {
            event,
            running: false,
            native_enabled: m.microphone_enabled,
            channels,
            pending: VecDeque::new(),
            released: VecDeque::new(),
            bytes: 0,
            gain: 1.,
            mic: Microphone::default(),
        },
    );
    let mut reply = Reply::object(handle);
    reply.data = words(&[48000, channels as u32, 2, 1]);
    let mut name = vec![0; 256];
    name[..8].copy_from_slice(b"DeviceIn");
    reply.write(out, name)
}
fn input<'a>(m: &'a mut Media, x: &mut Context<'_>) -> Result<&'a mut Input, Error> {
    let input = m
        .inputs
        .get_mut(&x.object)
        .ok_or(Error::Malformed("audio input object"))?;
    input.poll(x.kernel, x.memory, x.now_ns)?;
    Ok(input)
}
fn command(m: &mut Media, r: &Request, x: &mut Context<'_>) -> Result<Reply, Error> {
    let now = x.now_ns;
    let input = input(m, x)?;
    match r.command_id {
        0 => Ok(Reply::data(words(&[u32::from(!input.running)]))),
        1 | 2 => {
            input.running(r.command_id == 1, now);
            Ok(Reply::success())
        }
        4 => Ok(Reply::copy(input.event)),
        6 => Ok(Reply::data(vec![u8::from(
            input.pending.iter().any(|p| p.tag == r.u64(0).unwrap()),
        )])),
        11 => Ok(Reply::data(words(&[input.pending.len() as u32]))),
        12 => {
            let gain = f32::from_bits(r.u32(0)?);
            if !gain.is_finite() || gain < 0.0 {
                return Err(Error::Malformed("audio input gain"));
            }
            input.gain = gain;
            Ok(Reply::success())
        }
        13 => Ok(Reply::data(input.gain.to_le_bytes().to_vec())),
        14 => {
            let pending = !input.pending.is_empty();
            for p in input.pending.drain(..) {
                input.released.push_back(p.tag);
            }
            input.bytes = 0;
            if pending {
                x.kernel.signal_event(input.event).map_err(Error::Kernel)?;
            }
            Ok(Reply::data(vec![u8::from(pending)]))
        }
        _ => unreachable!(),
    }
}
fn append(m: &mut Media, r: &Request, x: &mut Context<'_>) -> Result<Reply, Error> {
    let descriptor = x.read_buffer(r.input()?, 40)?;
    if descriptor.len() != 40 || read_u64(&descriptor, 0)? != 0 || read_u64(&descriptor, 32)? != 0 {
        return Err(Error::Malformed("audio input descriptor"));
    }
    let address = read_u64(&descriptor, 8)?;
    let capacity = read_u64(&descriptor, 16)?;
    let size = read_u64(&descriptor, 24)?;
    let size = if size == 0 { capacity } else { size };
    let channels = m
        .inputs
        .get(&x.object)
        .ok_or(Error::Malformed("audio input object"))?
        .channels;
    if size == 0
        || size > capacity
        || size > MAX_BYTES as u64
        || !size.is_multiple_of((channels * 2) as u64)
    {
        return Err(Error::Malformed("audio input sample size"));
    }
    x.memory.check_access(address, size as usize, 2)?;
    let tag = r.u64(0)?;
    let now = x.now_ns;
    let input = input(m, x)?;
    if input.pending.len() + input.released.len() >= 64
        || input.bytes + size as usize > MAX_BYTES
        || input.pending.iter().any(|p| p.tag == tag)
        || input.released.contains(&tag)
    {
        return Err(Error::Malformed("audio input queue limit or duplicate tag"));
    }
    let duration = size * 1_000_000_000 / (48000 * channels as u64 * 2);
    let deadline = input.running.then(|| {
        input
            .pending
            .back()
            .and_then(|p| p.deadline)
            .unwrap_or(now)
            .max(now)
            .saturating_add(duration)
    });
    input.pending.push_back(Pending {
        tag,
        address,
        bytes: size as usize,
        remaining: duration,
        deadline,
    });
    input.bytes += size as usize;
    Ok(Reply::success())
}
fn released(m: &mut Media, r: &Request, x: &mut Context<'_>) -> Result<Reply, Error> {
    let out = r.output()?;
    if out.len > MAX_BYTES as u64 || !out.len.is_multiple_of(8) {
        return Err(Error::Malformed("audio input released array"));
    }
    x.memory.check_access(out.address, out.len as usize, 2)?;
    let input = input(m, x)?;
    let count = input.released.len().min(out.len as usize / 8);
    let data = input
        .released
        .iter()
        .take(count)
        .flat_map(|t| t.to_le_bytes())
        .collect();
    let reply = Reply::data(words(&[count as u32])).write(out, data)?;
    input.released.drain(..count);
    if input.released.is_empty() {
        x.kernel.clear_event(input.event).map_err(Error::Kernel)?;
    }
    Ok(reply)
}
