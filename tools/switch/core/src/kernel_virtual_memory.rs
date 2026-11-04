//! Bounded physical allocations and same-process loader code aliases.
use super::*;
const ALIAS_START: u64 = 0x500000000;
const ALIAS_END: u64 = ALIAS_START + 0x40000000;
const CODE_START: u64 = 0x600000000;
const CODE_END: u64 = 0x700000000;
fn range(address: u64, size: u64) -> Result<std::ops::Range<u64>, u64> {
    if !address.is_multiple_of(4096) {
        return Err(INVALID_ADDRESS);
    }
    if size == 0 || !size.is_multiple_of(4096) {
        return Err(INVALID_SIZE);
    }
    Ok(address..address.checked_add(size).ok_or(INVALID_ADDRESS)?)
}
impl Kernel {
    pub(super) fn dispatch_virtual_memory(
        &mut self,
        memory: &mut dyn GuestMemory,
        svc: u16,
        r: &[u64; 31],
    ) -> Result<Dispatch, u64> {
        match svc {
            0x2c | 0x2d => {
                let area = range(r[0], r[1])?;
                let size = usize::try_from(r[1]).map_err(|_| INVALID_SIZE)?;
                if area.start < ALIAS_START || area.end > ALIAS_END {
                    return Err(0xdc01);
                }
                if svc == 0x2c {
                    memory
                        .map_zeroed(area.start, size, 3)
                        .map_err(|e| match e {
                            memory::Error::Capacity => LIMIT_REACHED,
                            _ => INVALID_MEMORY_STATE,
                        })?;
                    self.physical_mappings.insert(area.start, size);
                } else {
                    if self.physical_mappings.get(&area.start) != Some(&size) {
                        return Err(INVALID_MEMORY_STATE);
                    }
                    memory
                        .unmap(area.start, size)
                        .map_err(|_| INVALID_MEMORY_STATE)?;
                    self.physical_mappings.remove(&area.start);
                }
            }
            0x77 | 0x78 => {
                let handle = u32::try_from(r[0]).map_err(|_| INVALID_HANDLE)?;
                if !matches!(self.lookup(handle), Some(Object::Process)) {
                    return Err(INVALID_HANDLE);
                }
                let destination = range(r[1], r[3])?;
                let source = range(r[2], r[3])?;
                let size = usize::try_from(r[3]).map_err(|_| INVALID_SIZE)?;
                if destination.start < CODE_START || destination.end > CODE_END {
                    return Err(0xdc01);
                }
                let key = (destination.start, source.start);
                if svc == 0x77 {
                    if source.start < HEAP
                        || source.end
                            > HEAP + u64::try_from(self.heap_size).map_err(|_| INVALID_SIZE)?
                    {
                        return Err(INVALID_MEMORY_STATE);
                    }
                    if self.objects.values().any(|e| matches!(e.object, Object::Transfer {address, size} if source.start < address + u64::try_from(size).unwrap_or(u64::MAX) && address < source.end)) { return Err(INVALID_MEMORY_STATE); }
                    memory
                        .check_access(source.start, size, 3)
                        .map_err(|_| INVALID_MEMORY_STATE)?;
                    memory
                        .alias(destination.start, source.start, size, 0)
                        .map_err(|_| INVALID_MEMORY_STATE)?;
                    if memory.protect(source.start, size, 0).is_err() {
                        memory
                            .unmap(destination.start, size)
                            .map_err(|_| INVALID_MEMORY_STATE)?;
                        return Err(INVALID_MEMORY_STATE);
                    }
                    memory
                        .set_state(destination.start, size, 8)
                        .map_err(|_| INVALID_MEMORY_STATE)?;
                    self.code_mappings.insert(key, size);
                } else {
                    if self.code_mappings.get(&key) != Some(&size) {
                        return Err(INVALID_MEMORY_STATE);
                    }
                    memory
                        .protect(source.start, size, 3)
                        .map_err(|_| INVALID_MEMORY_STATE)?;
                    memory
                        .unmap(destination.start, size)
                        .map_err(|_| INVALID_MEMORY_STATE)?;
                    self.code_mappings.remove(&key);
                }
            }
            0x73 => {
                let handle = u32::try_from(r[0]).map_err(|_| INVALID_HANDLE)?;
                if !matches!(self.lookup(handle), Some(Object::Process)) {
                    return Err(INVALID_HANDLE);
                }
                let area = range(r[1], r[2])?;
                let size = usize::try_from(r[2]).map_err(|_| INVALID_SIZE)?;
                let permissions = match r[3] {
                    0 => 0,
                    1 => 1,
                    3 => 3,
                    5 => 5,
                    _ => return Err(INVALID_COMBINATION),
                };
                if !self.code_mappings.iter().any(|((destination, _), mapped)| {
                    area.start >= *destination
                        && area.end <= *destination + u64::try_from(*mapped).unwrap_or(0)
                }) {
                    return Err(INVALID_MEMORY_STATE);
                }
                memory
                    .protect(area.start, size, permissions)
                    .map_err(|_| INVALID_MEMORY_STATE)?;
                memory
                    .set_state(area.start, size, if permissions == 3 { 9 } else { 8 })
                    .map_err(|_| INVALID_MEMORY_STATE)?;
            }
            _ => return Err(INVALID_ENUM),
        }
        Ok(ret(0))
    }
}
