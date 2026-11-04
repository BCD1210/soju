//! Prepare CPU writes in execution order; root owns atomic memory publication.
use super::*;
use gpu::Access as _;
pub(super) fn prepare(
    state: &gpu::ExecutionState,
    syncpoint: u32,
    access: &submit::Access<'_>,
    protected: &[(u64, u64)],
) -> Result<Vec<MemoryWrite>, Error> {
    if state.inflight_work {
        return Err(Error::Malformed("GPU work remains in flight"));
    }
    let mut writes = Vec::new();
    let mut increment = false;
    for action in &state.pending_actions {
        match action {
            gpu::Action::Write {
                address,
                bytes,
                kind,
            } => {
                if increment || bytes.is_empty() || !access.check(*address, bytes.len(), *kind, 3) {
                    return Err(Error::Malformed("GPU write contract"));
                }
                let (guest, _) = access.space.resolve(*address, bytes.len())?;
                validate_target(guest, bytes.len(), protected)?;
                writes.push(MemoryWrite {
                    address: guest,
                    bytes: bytes.clone(),
                });
            }
            gpu::Action::Copy { chunks } => {
                if increment || chunks.is_empty() || chunks.len() > 65536 {
                    return Err(Error::Malformed("GPU copy contract"));
                }
                let prior_writes = writes.len();
                let mut total = 0usize;
                let mut sources = Vec::with_capacity(chunks.len());
                for &(src, dst, size) in chunks {
                    total = total
                        .checked_add(size)
                        .filter(|&n| n <= 8 * 1024 * 1024)
                        .ok_or(Error::Malformed("GPU copy byte limit"))?;
                    if size == 0
                        || !access.check(src, size, 0, 1)
                        || !access.check(dst, size, 0xfe, 3)
                    {
                        return Err(Error::Malformed("GPU copy ranges"));
                    }
                    sources.push((access.space.resolve(src, size)?.0, size));
                }
                let start = sources
                    .iter()
                    .map(|v| v.0)
                    .min()
                    .ok_or(Error::Malformed("GPU copy sources"))?;
                let end = sources
                    .iter()
                    .map(|v| v.0 + u64::try_from(v.1).unwrap_or(u64::MAX))
                    .max()
                    .ok_or(Error::Malformed("GPU copy sources"))?;
                for ((_, dst, size), (source, _)) in chunks.iter().zip(sources) {
                    let (guest, _) = access.space.resolve(*dst, *size)?;
                    if submit::overlaps(
                        (
                            guest,
                            u64::try_from(*size).map_err(|_| Error::Malformed("GPU copy size"))?,
                        ),
                        (start, end - start),
                    ) {
                        return Err(Error::Malformed("overlapping GPU copy"));
                    }
                    validate_target(guest, *size, protected)?;
                    let mut bytes = vec![0; *size];
                    access.memory.read(source, &mut bytes)?;
                    overlay(source, &mut bytes, &writes[..prior_writes])?;
                    writes.push(MemoryWrite {
                        address: guest,
                        bytes,
                    });
                }
            }
            gpu::Action::Increment {
                syncpoint: id,
                increment: amount,
                wait,
            } => {
                if increment || *id != syncpoint || *amount != 1 || *wait {
                    return Err(Error::Malformed("GPU syncpoint operation"));
                }
                increment = true;
            }
            gpu::Action::Barrier => {}
        }
    }
    if !increment {
        return Err(Error::Malformed("GPU submission missing increment"));
    }
    Ok(writes)
}
pub(super) fn validate_target(
    address: u64,
    size: usize,
    protected: &[(u64, u64)],
) -> Result<(), Error> {
    let size = u64::try_from(size).map_err(|_| Error::Malformed("GPU write size"))?;
    if protected
        .iter()
        .any(|&range| submit::overlaps((address, size), range))
    {
        return Err(Error::Malformed(
            "GPU write overlaps submission control data",
        ));
    }
    Ok(())
}
fn overlay(address: u64, bytes: &mut [u8], writes: &[MemoryWrite]) -> Result<(), Error> {
    let end = address
        .checked_add(u64::try_from(bytes.len()).map_err(|_| Error::Malformed("GPU read size"))?)
        .ok_or(Error::Malformed("GPU read overflow"))?;
    for write in writes {
        let write_end = write
            .address
            .checked_add(
                u64::try_from(write.bytes.len()).map_err(|_| Error::Malformed("GPU write size"))?,
            )
            .ok_or(Error::Malformed("GPU write overflow"))?;
        let start = address.max(write.address);
        let finish = end.min(write_end);
        if start < finish {
            let dst = usize::try_from(start - address)
                .map_err(|_| Error::Malformed("GPU overlay offset"))?;
            let src = usize::try_from(start - write.address)
                .map_err(|_| Error::Malformed("GPU overlay offset"))?;
            let len = usize::try_from(finish - start)
                .map_err(|_| Error::Malformed("GPU overlay length"))?;
            bytes[dst..dst + len].copy_from_slice(&write.bytes[src..src + len]);
        }
    }
    Ok(())
}
