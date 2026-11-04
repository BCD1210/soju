//! Bounded deko compute storage descriptors and code discovery.
use super::*;
use soju_switch_shader::msl::{GlobalMemory, Options};

pub(super) fn register(
    dispatch: &soju_switch_gpu::compute::Dispatch,
    memory: &Memory,
    mappings: &[(u64, u64, Vec<u8>)],
    backend: &mut Backend,
) -> Result<(), Error> {
    let address = dispatch.program();
    let (base, _, allocation) = mappings
        .iter()
        .find(|(base, _, bytes)| address >= *base && address - *base < bytes.len() as u64)
        .ok_or(Error::Malformed("compute code outside GPU mappings"))?;
    let available = (allocation.len() - (address - base) as usize).min(1024 * 1024);
    let size = programs::code_extent(memory.read(address, available).map_err(gpu_error)?)?;
    // deko ComputeDriverCbuf: 32-byte dimensions, 16 uniform addresses,
    // 32 texture handles, 8 image handles, then 16 storage descriptors.
    let driver = dispatch
        .constant(0)
        .ok_or(Error::Malformed("compute driver cbuf missing"))?;
    if driver.size < 576 {
        return Err(Error::Malformed("compute driver cbuf extent"));
    }
    let table = memory.read(driver.address + 320, 256).map_err(gpu_error)?;
    let mut global = None;
    for entry in table.chunks_exact(16) {
        let base_address = u64::from_le_bytes(
            entry[..8]
                .try_into()
                .map_err(|_| Error::Malformed("compute storage address"))?,
        );
        let bytes = read_u32(entry, 8)?;
        if bytes == 0 {
            continue;
        }
        if global.is_some() {
            return Err(Error::Malformed(
                "multiple compute storage buffers unsupported",
            ));
        }
        memory
            .read(base_address, bytes as usize)
            .map_err(gpu_error)?;
        global = Some(GlobalMemory {
            buffer: 29,
            base_address,
            bytes,
        });
    }
    let global_memory = Some(global.ok_or(Error::Malformed("compute storage buffer missing"))?);
    let mut constant_buffer_bytes = [0; 32];
    for (slot, bytes) in constant_buffer_bytes.iter_mut().enumerate().take(8) {
        *bytes = dispatch.constant(slot).map_or(0, |cb| cb.size);
    }
    backend
        .compute_program(
            address,
            size,
            Options {
                constant_buffer_bytes,
                global_memory,
                ..Options::default()
            },
        )
        .map_err(gpu_error)
}
