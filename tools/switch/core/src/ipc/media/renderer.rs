//! Bounded PCM16 renderer using the public libnx audren update structures.
use super::*;
use std::collections::VecDeque;
const PERIOD: u64 = 5_000_000;
const MAX_WAVE: usize = 8 * 1024 * 1024;
#[derive(Clone, Default)]
struct Wave {
    pcm: Vec<i16>,
    position: usize,
    looping: bool,
}
#[derive(Clone, Default)]
struct Voice {
    waves: [Option<Wave>; 4],
    head: usize,
    channels: usize,
    gains: [[f32; 2]; 2],
    volume: f32,
    state: u8,
    played: u64,
    consumed: u32,
}
#[derive(Clone)]
pub(super) struct Renderer {
    pub running: bool,
    pub event: Handle,
    pub work_handle: Handle,
    revision: u32,
    voices: Vec<Voice>,
    next: u64,
    pcm: VecDeque<Vec<u8>>,
}
impl Renderer {
    pub fn take_pcm(&mut self) -> Option<Vec<u8>> {
        self.pcm.pop_front()
    }
    pub fn poll(&mut self, kernel: &mut Kernel, now: u64) -> Result<(), Error> {
        if !self.running {
            return Ok(());
        }
        // Bound catch-up and host queue growth after long host stalls.
        for _ in 0..8 {
            if now < self.next {
                break;
            }
            self.next = self.next.saturating_add(PERIOD);
            let mut mix = [0f32; 480];
            for voice in &mut self.voices {
                if voice.state != 0 {
                    continue;
                }
                for frame in 0..240 {
                    let Some(wave) = &mut voice.waves[voice.head] else {
                        break;
                    };
                    for channel in 0..voice.channels {
                        let sample = f32::from(wave.pcm[wave.position + channel]) * voice.volume;
                        for dest in 0..2 {
                            mix[frame * 2 + dest] += sample * voice.gains[channel][dest];
                        }
                    }
                    wave.position += voice.channels;
                    voice.played += 1;
                    if wave.position == wave.pcm.len() {
                        if wave.looping {
                            wave.position = 0;
                        } else {
                            voice.waves[voice.head] = None;
                            voice.head = (voice.head + 1) % 4;
                            voice.consumed = voice.consumed.saturating_add(1);
                        }
                    }
                }
            }
            if self.pcm.len() == 200 {
                self.pcm.pop_front();
            }
            self.pcm.push_back(
                mix.iter()
                    .map(|v| (*v).round().clamp(-32768., 32767.) as i16)
                    .flat_map(i16::to_le_bytes)
                    .collect(),
            );
            kernel.signal_event(self.event).map_err(Error::Kernel)?;
        }
        Ok(())
    }
}
const fn cmd(
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
pub(super) const SERVICE: &[CommandSpec<Media>] = &[cmd(0, 72, open), cmd(1, 52, size)];
pub(super) const RENDERER: &[CommandSpec<Media>] = &[
    cmd(0, 0, rate),
    cmd(1, 0, samples),
    cmd(2, 0, mix_count),
    cmd(3, 0, state),
    cmd(4, 0, update),
    cmd(5, 0, start),
    cmd(6, 0, stop),
    cmd(7, 0, event),
    cmd(10, 0, update),
];
fn config(req: &Request) -> Result<(usize, u32), Error> {
    let voices = req.u32(16)? as usize;
    let revision = req.u32(48)?;
    if req.u32(0)? != 48000
        || req.u32(4)? != 240
        || req.u32(8)? != 2
        || req.u32(12)? != 0
        || !(1..=64).contains(&voices)
        || req.u32(20)? != 1
        || req.u32(24)? != 0
        || req.data[28..48].iter().any(|b| *b != 0)
        || ![0x31564552, 0x32564552, 0x33564552, 0x34564552].contains(&revision)
    {
        return Err(Error::Malformed(
            "audren configuration: requires 48kHz PCM final stereo mix",
        ));
    }
    Ok((voices, revision))
}
fn size(_: &mut Media, req: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    config(req)?;
    Ok(Reply::data(0x1000u64.to_le_bytes().to_vec()))
}
fn open(media: &mut Media, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
    let (voices, revision) = config(req)?;
    if req.copy_handles.len() != 2
        || req.copy_handles[1] != 0xffff8001
        || !req.move_handles.is_empty()
    {
        return Err(Error::Malformed("audren open handles"));
    }
    let id = ctx
        .kernel
        .object_id(req.copy_handles[0])
        .map_err(Error::Kernel)?;
    let transfer = ctx
        .kernel
        .transfer(id)
        .ok_or(Error::Malformed("audren transfer memory"))?;
    if req.u64(56)? != transfer.1 as u64 || transfer.1 < 4096 {
        return Err(Error::Malformed("audren work buffer size"));
    }
    let work_handle = ctx
        .kernel
        .duplicate_handle(req.copy_handles[0])
        .map_err(Error::Kernel)?;
    let event = match ctx.kernel.register_event(false) {
        Ok(event) => event,
        Err(error) => {
            ctx.kernel
                .close_handle(ctx.memory, work_handle)
                .map_err(Error::Kernel)?;
            return Err(Error::Kernel(error));
        }
    };
    let handle = match media.session(Interface::Renderer, ctx) {
        Ok(handle) => handle,
        Err(error) => {
            ctx.kernel
                .close_handle(ctx.memory, work_handle)
                .map_err(Error::Kernel)?;
            ctx.kernel
                .close_handle(ctx.memory, event)
                .map_err(Error::Kernel)?;
            return Err(error);
        }
    };
    let id = ctx.kernel.object_id(handle).map_err(Error::Kernel)?;
    media.renderers.insert(
        id,
        Renderer {
            work_handle,
            running: false,
            event,
            revision,
            voices: vec![Voice::default(); voices],
            next: 0,
            pcm: VecDeque::new(),
        },
    );
    Ok(Reply::object(handle))
}
fn renderer<'a>(media: &'a mut Media, ctx: &Context<'_>) -> Result<&'a mut Renderer, Error> {
    media
        .renderers
        .get_mut(&ctx.object)
        .ok_or(Error::Malformed("audren object"))
}
fn rate(_: &mut Media, _: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    Ok(Reply::data(words(&[48000])))
}
fn samples(_: &mut Media, _: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    Ok(Reply::data(words(&[240])))
}
fn mix_count(_: &mut Media, _: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    Ok(Reply::data(words(&[2])))
}
fn state(m: &mut Media, _: &Request, c: &mut Context<'_>) -> Result<Reply, Error> {
    Ok(Reply::data(words(&[u32::from(!renderer(m, c)?.running)])))
}
fn event(m: &mut Media, _: &Request, c: &mut Context<'_>) -> Result<Reply, Error> {
    Ok(Reply::copy(renderer(m, c)?.event))
}
fn start(m: &mut Media, _: &Request, c: &mut Context<'_>) -> Result<Reply, Error> {
    let r = renderer(m, c)?;
    if !r.running {
        r.running = true;
        r.next = c.now_ns.saturating_add(PERIOD);
    }
    Ok(Reply::success())
}
fn stop(m: &mut Media, _: &Request, c: &mut Context<'_>) -> Result<Reply, Error> {
    let r = renderer(m, c)?;
    r.running = false;
    r.pcm.clear();
    Ok(Reply::success())
}
fn float(data: &[u8], at: usize) -> Result<f32, Error> {
    let v = f32::from_bits(read_u32(data, at)?);
    if !v.is_finite() || v.abs() > 16. {
        return Err(Error::Malformed("audren gain"));
    }
    Ok(v)
}
fn put(data: &mut [u8], at: usize, v: u32) {
    data[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
fn update(media: &mut Media, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
    let raw = ctx.read_buffer(req.input()?, 1024 * 1024)?;
    let old = renderer(media, ctx)?;
    let n = old.voices.len();
    let pools = n * 4;
    let sizes = [16, pools * 32, n * 368, n * 112, 0, 2352, 320, 16];
    let total = 64 + sizes.iter().sum::<usize>();
    if raw.len() < total
        || read_u32(&raw, 0)? != old.revision
        || read_u32(&raw, 60)? as usize != total
    {
        return Err(Error::Malformed("audren update header"));
    }
    for (i, size) in sizes.iter().enumerate() {
        if read_u32(&raw, 4 + i * 4)? as usize != *size {
            return Err(Error::Malformed("audren section size"));
        }
    }
    if read_u32(&raw, 64)? != old.revision || read_u64(&raw, 72)? != 0 {
        return Err(Error::Malformed("audren behavior"));
    }
    let mut next = old.clone();
    let out_sizes = [176, pools * 16, n * 16, 0, 0, 0, 32, 16];
    let mut out = vec![0u8; 64 + out_sizes.iter().sum::<usize>()];
    let out_total = out.len() as u32;
    put(&mut out, 0, next.revision);
    put(&mut out, 60, out_total);
    for (i, size) in out_sizes.iter().enumerate() {
        put(&mut out, 4 + i * 4, *size as u32);
    }
    let mut attached = Vec::new();
    for i in 0..pools {
        let at = 80 + i * 32;
        let address = read_u64(&raw, at)?;
        let size = read_u64(&raw, at + 8)?;
        let state = read_u32(&raw, at + 16)?;
        let state = match state {
            0 | 1 | 3 | 6 => state,
            2 => 3,
            4 | 5 => 5,
            _ => return Err(Error::Malformed("audren pool state")),
        };
        if state == 5 {
            if address % 4096 != 0 || size == 0 || size % 4096 != 0 || size > MAX_WAVE as u64 {
                return Err(Error::Malformed("audren pool range"));
            }
            ctx.memory.check_access(address, size as usize, 1)?;
            attached.push((
                address,
                address
                    .checked_add(size)
                    .ok_or(Error::Malformed("audren pool overflow"))?,
            ));
        }
        put(&mut out, 64 + i * 16, state);
    }
    let channels = 80 + pools * 32;
    let voices = channels + n * 112;
    let mix = voices + n * 368;
    let sink = mix + 2352;
    if raw[mix + 12] != 1
        || read_u32(&raw, mix + 4)? != 48000
        || read_u32(&raw, mix + 8)? != 2
        || read_u32(&raw, mix + 16)? != 0
        || raw[sink] != 1
        || raw[sink + 1] != 1
        || read_u32(&raw, sink + 288)? != 2
        || raw[sink + 292..sink + 294] != [0, 1]
        || raw[sink + 299] != 0
    {
        return Err(Error::Malformed("audren final mix or device sink"));
    }
    let name = &raw[sink + 32..sink + 287];
    if name.split(|b| *b == 0).next() != Some(b"MainAudioOut".as_slice()) {
        return Err(Error::Malformed("audren device name"));
    }
    let master = float(&raw, mix)?;
    let mut wave_bytes: usize = next
        .voices
        .iter()
        .flat_map(|v| v.waves.iter().flatten())
        .map(|w| w.pcm.len() * 2)
        .sum();
    for i in 0..n {
        let at = voices + i * 368;
        if raw[at + 9] == 0 {
            wave_bytes -= next.voices[i]
                .waves
                .iter()
                .flatten()
                .map(|w| w.pcm.len() * 2)
                .sum::<usize>();
            next.voices[i] = Voice::default();
            continue;
        }
        let count = read_u32(&raw, at + 24)? as usize;
        if read_u32(&raw, at)? as usize != i
            || raw[at + 11] != 2
            || read_u32(&raw, at + 12)? != 48000
            || !(1..=2).contains(&count)
            || float(&raw, at + 28)? != 1.
            || raw[at + 36] != 0
            || raw[at + 48] != 0
            || read_u32(&raw, at + 88)? != 0
            || read_u32(&raw, at + 92)? != u32::MAX
            || read_u32(&raw, at + 60)? > 4
            || raw[at + 10] > 2
        {
            return Err(Error::Malformed(
                "audren voice requires PCM16 48kHz unfiltered final mix",
            ));
        }
        let v = &mut next.voices[i];
        if raw[at + 8] == 0 && v.channels != 0 && v.channels != count {
            return Err(Error::Malformed(
                "audren channel count changed without voice reset",
            ));
        }
        if raw[at + 8] != 0 {
            wave_bytes -= v
                .waves
                .iter()
                .flatten()
                .map(|w| w.pcm.len() * 2)
                .sum::<usize>();
            *v = Voice::default();
            v.head = u16::from_le_bytes(raw[at + 64..at + 66].try_into().unwrap()) as usize;
            if v.head >= 4 {
                return Err(Error::Malformed("audren wave head"));
            }
        }
        v.channels = count;
        v.state = raw[at + 10];
        v.volume = float(&raw, at + 32)? * master;
        if read_u64(&raw, at + 72)? != 0 || read_u64(&raw, at + 80)? != 0 {
            return Err(Error::Malformed("audren extra voice parameters"));
        }
        for ch in 0..count {
            let id = read_u32(&raw, at + 320 + ch * 4)? as usize;
            if id >= n
                || read_u32(&raw, channels + id * 112)? as usize != id
                || raw[channels + id * 112 + 100] != 1
            {
                return Err(Error::Malformed("audren channel"));
            }
            for dest in 2..24 {
                if float(&raw, channels + id * 112 + 4 + dest * 4)? != 0. {
                    return Err(Error::Malformed("audren non-stereo routing"));
                }
            }
            for dest in 0..2 {
                v.gains[ch][dest] = float(&raw, channels + id * 112 + 4 + dest * 4)?;
            }
        }
        for slot in 0..4 {
            let w = at + 96 + slot * 56;
            let address = read_u64(&raw, w)?;
            if address == 0 || raw[w + 26] != 0 {
                continue;
            }
            let size = read_u64(&raw, w + 8)?;
            let begin = read_u32(&raw, w + 16)? as usize;
            let end = read_u32(&raw, w + 20)? as usize;
            if size > MAX_WAVE as u64
                || begin >= end
                || end
                    .checked_mul(count * 2)
                    .is_none_or(|end| end > size as usize)
                || !attached.iter().any(|(a, b)| {
                    address >= *a && address.checked_add(size).is_some_and(|end| end <= *b)
                })
                || read_u64(&raw, w + 32)? != 0
                || read_u64(&raw, w + 40)? != 0
            {
                return Err(Error::Malformed("audren wave range"));
            }
            wave_bytes = wave_bytes - v.waves[slot].as_ref().map_or(0, |w| w.pcm.len() * 2)
                + (end - begin) * count * 2;
            if wave_bytes > MAX_WAVE {
                return Err(Error::Malformed("audren wave memory budget"));
            }
            let mut bytes = vec![0; (end - begin) * count * 2];
            ctx.memory
                .read(address + (begin * count * 2) as u64, &mut bytes)?;
            v.waves[slot] = Some(Wave {
                pcm: bytes
                    .chunks_exact(2)
                    .map(|b| i16::from_le_bytes([b[0], b[1]]))
                    .collect(),
                position: 0,
                looping: raw[w + 24] != 0,
            });
        }
        let dest = 64 + pools * 16 + i * 16;
        out[dest..dest + 8].copy_from_slice(&v.played.to_le_bytes());
        put(&mut out, dest + 8, v.consumed);
    }
    let target = output_buffer(req, ctx, out.len())?;
    let reply = Reply::success().write(target, out)?;
    media.renderers.insert(ctx.object, next);
    Ok(reply)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (Renderer, Kernel) {
        let boot = crate::nro::Homebrew {
            regions: vec![],
            pc: 0,
            sp: 0,
            x0: 0,
            x1: 0,
            x30: 0,
            tpidrro_el0: 0,
        };
        let mut kernel = Kernel::new(&boot);
        let event = kernel.register_event(false).unwrap();
        (
            Renderer {
                running: true,
                event,
                work_handle: 0,
                revision: 0x34564552,
                voices: vec![Voice::default()],
                next: PERIOD,
                pcm: VecDeque::new(),
            },
            kernel,
        )
    }
    #[test]
    fn pcm_routes_consumes_once_and_reports_sample_position() {
        let (mut r, mut kernel) = setup();
        r.voices[0] = Voice {
            channels: 1,
            gains: [[1., 0.5], [0., 0.]],
            volume: 1.,
            waves: [
                Some(Wave {
                    pcm: vec![1000, -1000],
                    position: 0,
                    looping: false,
                }),
                None,
                None,
                None,
            ],
            ..Voice::default()
        };
        r.poll(&mut kernel, PERIOD - 1).unwrap();
        assert!(r.take_pcm().is_none());
        r.poll(&mut kernel, PERIOD).unwrap();
        let pcm = r.take_pcm().unwrap();
        assert_eq!(
            &pcm[..8],
            &[1000i16, 500, -1000, -500]
                .into_iter()
                .flat_map(i16::to_le_bytes)
                .collect::<Vec<_>>()
        );
        assert!(pcm[8..].iter().all(|v| *v == 0));
        assert_eq!((r.voices[0].played, r.voices[0].consumed), (2, 1));
        r.poll(&mut kernel, PERIOD * 2).unwrap();
        assert!(r.take_pcm().unwrap().iter().all(|v| *v == 0));
        assert_eq!(r.voices[0].consumed, 1);
    }
    #[test]
    fn stereo_loop_pause_and_clipping() {
        let (mut r, mut kernel) = setup();
        r.voices[0] = Voice {
            channels: 2,
            gains: [[1., 0.], [0., 1.]],
            volume: 2.,
            waves: [
                Some(Wave {
                    pcm: vec![30000, -30000],
                    position: 0,
                    looping: true,
                }),
                None,
                None,
                None,
            ],
            ..Voice::default()
        };
        r.poll(&mut kernel, PERIOD).unwrap();
        let expected = [32767i16, -32768]
            .into_iter()
            .flat_map(i16::to_le_bytes)
            .collect::<Vec<_>>();
        assert!(r.take_pcm().unwrap().chunks_exact(4).all(|p| p == expected));
        assert_eq!((r.voices[0].played, r.voices[0].consumed), (240, 0));
        r.voices[0].state = 2;
        r.poll(&mut kernel, PERIOD * 2).unwrap();
        assert!(r.take_pcm().unwrap().iter().all(|v| *v == 0));
        assert_eq!(r.voices[0].played, 240);
        r.voices[0].state = 0;
        r.poll(&mut kernel, PERIOD * 3).unwrap();
        assert_eq!(r.voices[0].played, 480);
    }
    #[test]
    fn update_rejects_bad_wave_without_mutating_renderer() {
        use crate::{
            memory::{MemoryImage, PAGE},
            nro::{BASE, Homebrew, Region, TLS},
        };
        let boot = Homebrew {
            regions: vec![Region {
                address: BASE,
                bytes: vec![0; PAGE * 8],
                permissions: 3,
            }],
            pc: BASE,
            sp: 0,
            x0: 0,
            x1: 0,
            x30: 0,
            tpidrro_el0: TLS,
        };
        let mut memory = MemoryImage::new(&boot).unwrap();
        let (renderer, mut kernel) = setup();
        let mut media = Media::default();
        media.renderers.insert(99, renderer);
        let sizes = [16, 128, 368, 112, 0, 2352, 320, 16];
        let mut raw = vec![0; 64 + sizes.iter().sum::<usize>()];
        let total = raw.len() as u32;
        put(&mut raw, 0, 0x34564552);
        put(&mut raw, 60, total);
        put(&mut raw, 64, 0x34564552);
        for (i, size) in sizes.iter().enumerate() {
            put(&mut raw, 4 + i * 4, *size as u32);
        }
        let pool = BASE + 0x4000;
        raw[80..88].copy_from_slice(&pool.to_le_bytes());
        raw[88..96].copy_from_slice(&4096u64.to_le_bytes());
        put(&mut raw, 96, 4);
        let channel = 208;
        put(&mut raw, channel + 4, 1f32.to_bits());
        put(&mut raw, channel + 8, 1f32.to_bits());
        raw[channel + 100] = 1;
        let voice = 320;
        raw[voice + 8] = 1;
        raw[voice + 9] = 1;
        raw[voice + 11] = 2;
        put(&mut raw, voice + 12, 48000);
        put(&mut raw, voice + 24, 1);
        put(&mut raw, voice + 28, 1f32.to_bits());
        put(&mut raw, voice + 32, 1f32.to_bits());
        put(&mut raw, voice + 60, 1);
        put(&mut raw, voice + 92, u32::MAX);
        let wave = voice + 96;
        raw[wave..wave + 8].copy_from_slice(&pool.to_le_bytes());
        raw[wave + 8..wave + 16].copy_from_slice(&4u64.to_le_bytes());
        put(&mut raw, wave + 20, 3);
        let mix = 688;
        put(&mut raw, mix, 1f32.to_bits());
        put(&mut raw, mix + 4, 48000);
        put(&mut raw, mix + 8, 2);
        raw[mix + 12] = 1;
        let sink = mix + 2352;
        raw[sink] = 1;
        raw[sink + 1] = 1;
        raw[sink + 32..sink + 44].copy_from_slice(b"MainAudioOut");
        put(&mut raw, sink + 288, 2);
        raw[sink + 293] = 1;
        let input = Buffer {
            kind: BufferKind::Send,
            address: BASE + 0x1000,
            len: raw.len() as u64,
            index: 0,
        };
        let output = Buffer {
            kind: BufferKind::Receive,
            address: BASE + 0x2000,
            len: 4096,
            index: 1,
        };
        let req = Request {
            protocol: Protocol::Cmif,
            kind: MessageKind::Request,
            command_id: 10,
            pid: None,
            buffers: vec![input, output],
            data: vec![],
            copy_handles: vec![],
            move_handles: vec![],
            domain_objects: vec![],
            objects: vec![],
            domain_target: None,
            token: 0,
        };
        memory.write(input.address, &raw).unwrap();
        let mut ctx = Context {
            kernel: &mut kernel,
            memory: &mut memory,
            object: 99,
            session: 0,
            client: ClientId(1),
            now_ns: 0,
            tls: TLS,
            nv: None,
        };
        assert!(update(&mut media, &req, &mut ctx).is_err());
        assert!(
            media.renderers[&99].voices[0]
                .waves
                .iter()
                .all(Option::is_none)
        );
        put(&mut raw, wave + 20, 2);
        ctx.memory.write(input.address, &raw).unwrap();
        ctx.memory.write(pool, &[1, 0, 2, 0]).unwrap();
        let reply = update(&mut media, &req, &mut ctx).unwrap();
        assert_eq!(reply.writes.len(), 1);
        assert_eq!(read_u32(&reply.writes[0].bytes, 64).unwrap(), 5);
        assert_eq!(
            media.renderers[&99].voices[0].waves[0]
                .as_ref()
                .unwrap()
                .pcm,
            vec![1, 2]
        );
        raw[voice + 8] = 0;
        put(&mut raw, voice + 24, 2);
        ctx.memory.write(input.address, &raw).unwrap();
        assert!(update(&mut media, &req, &mut ctx).is_err());
        put(&mut raw, voice + 24, 1);
        raw[voice + 11] = 6;
        ctx.memory.write(input.address, &raw).unwrap();
        assert!(update(&mut media, &req, &mut ctx).is_err());
        assert_eq!(
            media.renderers[&99].voices[0].waves[0]
                .as_ref()
                .unwrap()
                .pcm,
            vec![1, 2]
        );
    }
}
