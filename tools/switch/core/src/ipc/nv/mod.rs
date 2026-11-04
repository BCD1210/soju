//! Session-owned NV device service and bounded CPU GPU execution.
mod commit;
mod control;
mod gpu;
mod ioctl;
mod memory;
mod metal;
mod nvmap;
mod queries;
mod service;
mod submit;
use super::types::*;
use crate::kernel::{Handle, ObjectId};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Device {
    Nvmap,
    Ctrl,
    CtrlGpu,
    Gpu,
    AsGpu,
}
#[derive(Clone)]
struct File {
    owner: ObjectId,
    device: Device,
}
#[derive(Clone)]
struct Allocation {
    owner: ObjectId,
    client: ClientId,
    view: NvBufferView,
    flags: u32,
}
#[derive(Clone, Default)]
struct Channel {
    execution: gpu::ExecutionState,
    completed: u32,
    faulted: bool,
    space: Option<u32>,
    nvmap: Option<u32>,
    capacity: Option<u32>,
    objects: u64,
    notifier: bool,
    priority: u32,
    zcull: Option<u64>,
    event: Option<Handle>,
}
#[derive(Default)]
pub struct Nv {
    pending: BTreeMap<u64, submit::Pending>,
    metal: BTreeMap<u32, metal::Renderer>,
    next_transaction: u64,
    next_fd: u32,
    sessions: BTreeSet<ObjectId>,
    files: BTreeMap<u32, File>,
    allocations: BTreeMap<u32, Allocation>,
    spaces: BTreeMap<u32, memory::AddressSpace>,
    channels: BTreeMap<u32, Channel>,
    transfers: BTreeMap<ObjectId, ObjectId>,
    release: Vec<ObjectId>,
    close: Vec<Handle>,
    generation: u64,
}
impl Nv {
    pub fn open(&mut self, name: &str, ctx: &mut Context<'_>) -> Result<Handle, Error> {
        if !matches!(name, "nvdrv:a" | "nvdrv") {
            return Err(Error::Malformed("unsupported NV service"));
        }
        let (handle, object) = ctx.new_session(Family::Nv)?;
        self.sessions.insert(object);
        Ok(handle)
    }
    pub fn dispatch(&mut self, request: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
        self.poll(ctx.kernel, ctx.memory, ctx.now_ns)?;
        if !self.sessions.contains(&ctx.object) {
            return Err(Error::Malformed("unknown NV session"));
        }
        dispatch_table(self, service::COMMANDS, request, ctx)
    }
    pub fn close_object(&mut self, object: ObjectId) {
        self.sessions.remove(&object);
        if let Some(id) = self.transfers.remove(&object) {
            self.release.push(id);
        }
        let fds: Vec<_> = self
            .files
            .iter()
            .filter(|(_, f)| f.owner == object)
            .map(|(&fd, _)| fd)
            .collect();
        for fd in fds {
            self.remove_fd(fd);
        }
        self.allocations.retain(|_, a| a.owner != object);
    }
    pub fn poll(
        &mut self,
        kernel: &mut crate::kernel::Kernel,
        memory: &mut dyn crate::memory::GuestMemory,
        _now_ns: u64,
    ) -> Result<(), Error> {
        while let Some(&id) = self.release.last() {
            kernel.release_object(memory, id).map_err(Error::Kernel)?;
            self.release.pop();
        }
        while let Some(&handle) = self.close.last() {
            kernel.close_handle(memory, handle).map_err(Error::Kernel)?;
            self.close.pop();
        }
        Ok(())
    }
    fn remove_fd(&mut self, fd: u32) {
        self.files.remove(&fd);
        self.metal.remove(&fd);
        if let Some(channel) = self.channels.remove(&fd)
            && let Some(event) = channel.event
        {
            self.close.push(event);
        }
        if !self.channels.values().any(|c| c.space == Some(fd)) {
            self.spaces.remove(&fd);
        }
        self.spaces.retain(|id, _| {
            self.files.contains_key(id) || self.channels.values().any(|c| c.space == Some(*id))
        });
    }
    fn file(&self, fd: u32, owner: ObjectId) -> Result<Device, Error> {
        self.files
            .get(&fd)
            .filter(|f| f.owner == owner)
            .map(|f| f.device)
            .ok_or(Error::Malformed("NV fd ownership"))
    }
    fn allocation(&self, id: u32, owner: ObjectId) -> Result<&Allocation, Error> {
        self.allocations
            .get(&id)
            .filter(|a| a.owner == owner)
            .ok_or(Error::Malformed("NV allocation ownership"))
    }
}
impl NvBufferLookup for Nv {
    fn resolve(&self, client: ClientId, id: u32) -> Result<NvBufferView, Error> {
        self.allocations
            .get(&id)
            .filter(|a| a.client == client && a.view.guest_address != 0)
            .map(|a| a.view)
            .ok_or(Error::Malformed("NV buffer client or lifetime"))
    }
}
fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests;
