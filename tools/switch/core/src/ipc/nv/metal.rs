//! Synchronous Metal submission boundary. Guest writes and fences publish through the IPC transaction.
use super::*;
use soju_switch_gpu::{backend::Backend, engine::Engine, memory::Memory};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
#[derive(Default)]
pub(super) struct Renderer {
    engine: Engine,
    bindings: BTreeMap<u32, u32>,
    directory: Option<PathBuf>,
    faulted: bool,
}
fn gpu_error(e: soju_switch_gpu::Error) -> Error {
    Error::Message(format!("Metal submission: {e}"))
}
impl Drop for Renderer {
    fn drop(&mut self) {
        if let Some(path) = &self.directory {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}
impl Renderer {
    pub(super) fn submit(
        &mut self,
        methods: &[gpu::Method],
        syncpoint: u32,
        access: &submit::Access<'_>,
        protected: &[(u64, u64)],
    ) -> Result<Vec<MemoryWrite>, Error> {
        if self.faulted {
            return Err(Error::Malformed("Metal channel faulted"));
        }
        // A failed native operation cannot be replayed: no completion or guest writes escape.
        self.faulted = true;
        let result = self.execute(methods, syncpoint, access, protected);
        if result.is_ok() {
            self.faulted = false;
        }
        result
    }
    fn execute(
        &mut self,
        methods: &[gpu::Method],
        syncpoint: u32,
        access: &submit::Access<'_>,
        protected: &[(u64, u64)],
    ) -> Result<Vec<MemoryWrite>, Error> {
        let mut memory = Memory::default();
        let mut original = Vec::new();
        let mut total = 0u64;
        for (&address, mapping) in &access.space.mappings {
            total = total
                .checked_add(mapping.size)
                .filter(|n| *n <= 512 * 1024 * 1024)
                .ok_or(Error::Malformed("Metal mapped memory limit"))?;
            let size = usize::try_from(mapping.size)
                .map_err(|_| Error::Malformed("Metal mapping size"))?;
            let (guest, _) = access.space.resolve(address, size)?;
            let mut bytes = vec![0; size];
            access.memory.read(guest, &mut bytes)?;
            memory.map(address, bytes.clone()).map_err(gpu_error)?;
            original.push((address, guest, bytes));
        }
        let mut backend = Backend::default();
        let mut incremented = false;
        for command in methods {
            let gpu::Method {
                subchannel,
                method,
                value,
                ..
            } = *command;
            if method == 0 {
                if !matches!(value, 0xb197 | 0xa140 | 0x902d | 0xb0b5 | 0xb1c0) {
                    return Err(Error::Message(format!(
                        "Metal unsupported class {value:#x}"
                    )));
                }
                self.bindings.insert(subchannel, value);
                continue;
            }
            let class = *self
                .bindings
                .get(&subchannel)
                .ok_or(Error::Malformed("Metal unbound subchannel"))?;
            if class == 0xb197 && method == 0x2c8 {
                if incremented || value != (1 << 20) | syncpoint {
                    return Err(Error::Message(format!(
                        "Metal syncpoint operation {value:#x}, expected {syncpoint}"
                    )));
                }
                incremented = true;
                continue;
            }
            if incremented {
                return Err(Error::Malformed("Metal method after completion"));
            }
            let snapshot = self
                .engine
                .method(class, method, value, &mut memory)
                .map_err(|e| {
                    Error::Message(format!(
                        "Metal method {class:#x}:{method:#x}={value:#x}: {e}"
                    ))
                })?;
            if let Some(snapshot) = snapshot {
                backend.prepare(&snapshot, &memory).map_err(gpu_error)?;
                // Read back before any later transfer, upload, or texture read.
                let dir = self.directory()?;
                backend.execute(&dir, &mut memory).map_err(gpu_error)?;
            }
        }
        if !incremented {
            return Err(Error::Malformed("Metal submission missing increment"));
        }
        let mut writes = Vec::new();
        for (address, guest, bytes) in original {
            let current = memory.read(address, bytes.len()).map_err(gpu_error)?;
            for (i, (before, after)) in bytes.chunks(4096).zip(current.chunks(4096)).enumerate() {
                if before != after {
                    let destination = guest + (i * 4096) as u64;
                    access.memory.check_access(destination, after.len(), 3)?;
                    super::commit::validate_target(destination, after.len(), protected)?;
                    writes.push(MemoryWrite {
                        address: destination,
                        bytes: after.to_vec(),
                    });
                }
            }
        }
        Ok(writes)
    }
    fn directory(&mut self) -> Result<PathBuf, Error> {
        if self.directory.is_none() {
            let path = std::env::temp_dir().join(format!(
                "soju-metal-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path)?;
            self.directory = Some(path);
        }
        Ok(self.directory.as_ref().unwrap().clone())
    }
}
