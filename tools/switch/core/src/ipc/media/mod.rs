//! VI, Binder and timed audio services over normalized IPC.
mod audio;
mod display;
mod frame;
mod renderer;
#[cfg(test)]
mod tests;

use super::types::*;
use crate::kernel::{Handle, Kernel, ObjectId};
use crate::memory::GuestMemory;
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
enum Interface {
    Vi,
    Display(ObjectId),
    Binder(ObjectId),
    Child,
    Audio,
    Output,
    RendererManager,
    Renderer,
}
#[derive(Default)]
pub struct Media {
    objects: BTreeMap<ObjectId, Interface>,
    displays: BTreeMap<ObjectId, display::Display>,
    audio: BTreeMap<ObjectId, audio::Output>,
    renderers: BTreeMap<ObjectId, renderer::Renderer>,
    retired_events: Vec<Handle>,
    frame: Option<Frame>,
    pub frame_conversion_time: std::time::Duration,
}
impl Media {
    pub fn open(&mut self, name: &str, ctx: &mut Context<'_>) -> Result<Handle, Error> {
        let interface = match name {
            "vi:m" => Interface::Vi,
            "audout:u" => Interface::Audio,
            "audren:u" => Interface::RendererManager,
            _ => return Err(Error::Malformed("unknown media service")),
        };
        self.session(interface, ctx)
    }
    fn session(&mut self, interface: Interface, ctx: &mut Context<'_>) -> Result<Handle, Error> {
        let (handle, id) = ctx.new_session(Family::Media)?;
        self.objects.insert(id, interface);
        Ok(handle)
    }
    pub fn dispatch(&mut self, request: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
        let table = match self
            .objects
            .get(&ctx.object)
            .ok_or(Error::Malformed("media object"))?
        {
            Interface::Vi => display::VI,
            Interface::Display(_) => display::DISPLAY,
            Interface::Binder(_) => display::BINDER,
            Interface::Child => &[],
            Interface::Audio => audio::SERVICE,
            Interface::Output => audio::OUTPUT,
            Interface::RendererManager => renderer::SERVICE,
            Interface::Renderer => renderer::RENDERER,
        };
        dispatch_table(self, table, request, ctx)
    }
    pub fn close_object(&mut self, id: ObjectId) {
        self.objects.remove(&id);
        if let Some(renderer) = self.renderers.remove(&id) {
            self.retired_events.push(renderer.event);
            self.retired_events.push(renderer.work_handle);
        }
        if let Some(output) = self.audio.remove(&id) {
            self.retired_events.push(output.event);
        }
        let stale: Vec<_> = self
            .displays
            .keys()
            .copied()
            .filter(|id| {
                !self.objects.values().any(|i|
            matches!(i, Interface::Display(owner) | Interface::Binder(owner) if owner == id))
            })
            .collect();
        for id in stale {
            if let Some(display) = self.displays.remove(&id) {
                self.retired_events.extend(display.events());
            }
        }
    }
    pub fn poll(
        &mut self,
        kernel: &mut Kernel,
        memory: &mut dyn GuestMemory,
        now_ns: u64,
    ) -> Result<(), Error> {
        for handle in self.retired_events.drain(..) {
            kernel.close_handle(memory, handle).map_err(Error::Kernel)?;
        }
        for display in self.displays.values_mut() {
            display.poll(kernel, now_ns)?;
        }
        for renderer in self.renderers.values_mut() {
            renderer.poll(kernel, now_ns)?;
        }
        for output in self.audio.values_mut() {
            output.poll(kernel, now_ns)?;
        }
        Ok(())
    }
    pub fn frame_ready(&self) -> bool {
        self.frame.is_some()
    }
    pub fn take_frame(&mut self) -> Option<Frame> {
        self.frame.take()
    }
    /// Drains PCM due for host submission. Timing and release continue without a host device.
    pub fn take_audio(&mut self) -> Option<AudioPacket> {
        self.audio
            .iter_mut()
            .find_map(|(&object, output)| output.take_pcm().map(|pcm| AudioPacket { object, pcm }))
            .or_else(|| {
                self.renderers.iter_mut().find_map(|(&object, renderer)| {
                    renderer.take_pcm().map(|pcm| AudioPacket { object, pcm })
                })
            })
    }
    pub fn audio_state(&self, object: ObjectId) -> Option<bool> {
        self.audio
            .get(&object)
            .map(|output| output.running)
            .or_else(|| self.renderers.get(&object).map(|r| r.running))
    }
    pub fn audio_running(&self, object: ObjectId) -> bool {
        self.audio_state(object) == Some(true)
    }
}
pub struct AudioPacket {
    pub object: ObjectId,
    pub pcm: Vec<u8>,
}
fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}
fn output_buffer(request: &Request, ctx: &Context<'_>, size: usize) -> Result<Buffer, Error> {
    let buffer = request.output()?;
    let len = u64::try_from(size).map_err(|_| Error::Malformed("output size"))?;
    let end = buffer
        .address
        .checked_add(len)
        .ok_or(Error::Malformed("output overflow"))?;
    if buffer.len < len || (buffer.address < ctx.tls.saturating_add(0x100) && ctx.tls < end) {
        return Err(Error::Malformed("output overlaps TLS or is too small"));
    }
    ctx.memory.check_access(buffer.address, size, 2)?;
    Ok(buffer)
}
