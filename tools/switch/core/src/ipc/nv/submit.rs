//! Submission snapshot, CPU staging, and post-reply fence publication.
use super::*;
use crate::memory::GuestMemory;
pub(super) struct Pending {
    pub fd: u32,
    pub state: gpu::ExecutionState,
    pub fence: u32,
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
    if count == 0
        || count > capacity.min(4096)
        || u64::from(count) * 8 != entries.len
        || flags != 0x104
        || read_u32(&args, 16)? != 0
        || read_u32(&args, 20)? != 1
    {
        return Err(Error::Malformed("CPU GPU submission contract"));
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
    let access = Access {
        space,
        memory: c.memory,
    };
    let mut staged = channel.execution.stage(&methods, &access)?;
    let writes = super::commit::prepare(&staged, fd, &access, &protected)?;
    let fence = channel.completed + 1;
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
        if rollback_failed {
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
