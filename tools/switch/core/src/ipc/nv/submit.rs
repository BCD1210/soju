//! Submission snapshot, CPU staging, and post-reply fence publication.
use super::*;
use crate::memory::GuestMemory;
pub(super) struct Pending {
    pub fd: u32,
    pub state: gpu::ExecutionState,
    pub fence: u32,
    pub metal: bool,
}
pub(super) struct Access<'a> {
    pub space: &'a memory::AddressSpace,
    pub memory: &'a dyn GuestMemory,
}
impl gpu::Access for Access<'_> {
    fn check(&self, address: u64, size: usize, kind: u32, permission: u8) -> bool {
        self.space.resolve(address, size).is_ok_and(|(guest, k)| {
            k == kind && self.memory.check_access(guest, size, permission).is_ok()
        })
    }
}
const COMMANDS: &[CommandSpec<Nv>] = &[CommandSpec {
    id: 0xc018481b,
    schema: Schema::at_least(8),
    handler: submit,
}];
pub(super) fn dispatch(nv: &mut Nv, r: &Request, c: &mut Context<'_>) -> Result<Reply, Error> {
    let _span = soju_switch_gpu::profile::span(soju_switch_gpu::profile::Stage::NvIoctl);
    let mut inner = r.clone();
    inner.command_id = r.u32(4)?;
    dispatch_table(nv, COMMANDS, &inner, c)
}
fn submit(nv: &mut Nv, r: &Request, c: &mut Context<'_>) -> Result<Reply, Error> {
    if (
        r.count(BufferKind::Pointer),
        r.count(BufferKind::Send),
        r.count(BufferKind::Receive),
    ) != (2, 2, 1)
        || !r.copy_handles.is_empty()
        || !r.move_handles.is_empty()
        || r.pid.is_some()
    {
        return Err(Error::Malformed("GPU submission descriptors"));
    }
    let fd = r.u32(0)?;
    if nv.file(fd, c.object)? != Device::Gpu {
        return Err(Error::Malformed("GPU submission device"));
    }
    let channel = nv
        .channels
        .get(&fd)
        .ok_or(Error::Malformed("GPU channel"))?;
    if channel.faulted || channel.completed == u32::MAX || nv.pending.values().any(|p| p.fd == fd) {
        return Err(Error::Malformed("GPU channel fault or pending submission"));
    }
    let capacity = channel
        .capacity
        .ok_or(Error::Malformed("GPU GPFIFO not initialized"))?;
    let space = nv
        .spaces
        .get(
            &channel
                .space
                .ok_or(Error::Malformed("GPU address space not bound"))?,
        )
        .ok_or(Error::Malformed("GPU address space"))?;
    let arguments = r.buffer(BufferKind::Send, 0)?;
    let entries = r.buffer(BufferKind::Send, 1)?;
    let output = r.buffer(BufferKind::Receive, 0)?;
    if arguments.len != 24 || output.len != 24 || entries.len > 32768 {
        return Err(Error::Malformed("GPU submission buffer sizes"));
    }
    c.memory.check_access(output.address, 24, 3)?;
    c.memory.check_access(c.tls, 256, 3)?;
    let args = c.read_buffer(arguments, 24)?;
    let entries_data = c.read_buffer(entries, 32768)?;
    let count = read_u32(&args, 8)?;
    let flags = read_u32(&args, 12)?;
    let metal = match std::env::var_os("SOJU_SWITCH_GPU").as_deref() {
        None => true,
        Some(value) if value == "metal" => true,
        Some(value) if value == "off" => false,
        Some(_) => return Err(Error::Malformed("SOJU_SWITCH_GPU must be metal or off")),
    };
    if count == 0
        || count > capacity.min(4096)
        || u64::from(count) * 8 != entries.len
        || !(flags == 0x104
            && read_u32(&args, 16)? == 0
            && (read_u32(&args, 20)? == 1 || metal && (1..=16).contains(&read_u32(&args, 20)?))
            || metal && flags == 4 && read_u32(&args, 16)? == 0 && read_u32(&args, 20)? == 0)
    {
        return Err(Error::Message(format!(
            "GPU submission contract: count={count} flags={flags:#x} fence={}:{}",
            read_u32(&args, 16)?,
            read_u32(&args, 20)?
        )));
    }
    let mut protected = vec![
        (arguments.address, arguments.len),
        (entries.address, entries.len),
        (output.address, output.len),
        (c.tls, 256),
    ];
    if overlaps((output.address, output.len), (c.tls, 256)) {
        return Err(Error::Malformed("GPU reply overlaps IPC state"));
    }
    let decode_span = soju_switch_gpu::profile::span(soju_switch_gpu::profile::Stage::Decode);
    let mut methods = Vec::new();
    let mut total_words = 0usize;
    for entry in entries_data.chunks_exact(8) {
        if let Some((address, words)) = gpu::decode_entry(read_u32(entry, 0)?, read_u32(entry, 4)?)?
        {
            total_words = total_words
                .checked_add(words)
                .filter(|&n| n <= 65536)
                .ok_or(Error::Malformed("GPU submission word limit"))?;
            let length = words * 4;
            let (guest, kind) = space.resolve(address, length)?;
            if kind != 0 {
                return Err(Error::Malformed("GPU command buffer is tiled"));
            }
            let mut bytes = vec![0; length];
            c.memory.read(guest, &mut bytes)?;
            let words = bytes
                .chunks_exact(4)
                .map(|b| read_u32(b, 0))
                .collect::<Result<Vec<_>, _>>()?;
            methods.extend(gpu::decode_packets(address, &words)?);
            protected.push((
                guest,
                u64::try_from(length).map_err(|_| Error::Malformed("GPU command length"))?,
            ));
        }
    }
    drop(decode_span);
    let access = Access {
        space,
        memory: c.memory,
    };
    let (mut staged, writes, increments) = if metal {
        let renderer = nv.metal.entry(fd).or_default();
        let (writes, increments) = renderer.submit(
            &methods,
            fd,
            (flags == 0x104).then_some(read_u32(&args, 20)?),
            &access,
            &protected,
        )?;
        (channel.execution.clone(), writes, increments)
    } else {
        let staged = channel.execution.stage(&methods, &access)?;
        let writes = super::commit::prepare(&staged, fd, &access, &protected)?;
        (staged, writes, 1)
    };
    let fence = channel
        .completed
        .checked_add(increments)
        .ok_or(Error::Malformed("GPU completion overflow"))?;
    let mut result = args;
    put32(&mut result, 16, fd);
    put32(&mut result, 20, fence);
    let token = nv
        .next_transaction
        .checked_add(1)
        .ok_or(Error::Malformed("NV transaction exhausted"))?;
    nv.next_transaction = token;
    staged.pending_actions.clear();
    nv.pending.insert(
        token,
        Pending {
            fd,
            state: staged,
            fence,
            metal,
        },
    );
    let mut reply = Reply::data(vec![0; 4]).write(output, result)?;
    reply.transaction = Some(token);
    reply.memory_writes = writes;
    Ok(reply)
}
pub(super) fn overlaps(a: (u64, u64), b: (u64, u64)) -> bool {
    u128::from(a.0) < u128::from(b.0) + u128::from(b.1)
        && u128::from(b.0) < u128::from(a.0) + u128::from(a.1)
}
impl Nv {
    pub fn finish_transaction(
        &mut self,
        token: u64,
        committed: bool,
        rollback_failed: bool,
        c: &mut Context<'_>,
    ) -> Result<(), Error> {
        let pending = self
            .pending
            .remove(&token)
            .ok_or(Error::Malformed("unknown NV transaction"))?;
        let channel = self
            .channels
            .get_mut(&pending.fd)
            .ok_or(Error::Malformed("closed GPU transaction channel"))?;
        if rollback_failed || (pending.metal && !committed) {
            channel.faulted = true;
            if let Some(event) = channel.event {
                c.kernel.signal_event(event).map_err(Error::Kernel)?;
            }
        }
        if committed && !rollback_failed {
            channel.execution = pending.state;
            channel.completed = pending.fence;
        }
        Ok(())
    }
}
