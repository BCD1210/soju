//! Synchronous Metal submission boundary. Guest writes and fences publish through the IPC transaction.
use super::*;
use soju_switch_gpu::{backend::Backend, engine::Engine, memory::Memory};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
#[derive(Default)]
pub(super) struct Renderer {
    engine: Engine,
    macros: gpu::mme::MacroUpload,
    bindings: BTreeMap<u32, u32>,
    twod: BTreeMap<u32, u32>,
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
        expected_increments: Option<u32>,
        access: &submit::Access<'_>,
        protected: &[(u64, u64)],
    ) -> Result<(Vec<MemoryWrite>, u32), Error> {
        if self.faulted {
            return Err(Error::Malformed("Metal channel faulted"));
        }
        // A failed native operation cannot be replayed: no completion or guest writes escape.
        self.faulted = true;
        let result = self.execute(methods, syncpoint, expected_increments, access, protected);
        if result.is_ok() {
            self.faulted = false;
        }
        result
    }
    fn execute(
        &mut self,
        methods: &[gpu::Method],
        syncpoint: u32,
        expected_increments: Option<u32>,
        access: &submit::Access<'_>,
        protected: &[(u64, u64)],
    ) -> Result<(Vec<MemoryWrite>, u32), Error> {
        if std::env::var_os("SOJU_SWITCH_TRACE_GPU").is_some() {
            for m in methods {
                eprintln!("gpu-method|{}|{:#x}|{:#x}", m.subchannel, m.method, m.value);
            }
        }
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
        let mut increments = 0u32;
        let mut queue: std::collections::VecDeque<_> = methods.iter().copied().collect();
        let mut processed = 0usize;
        while let Some(command) = queue.pop_front() {
            processed += 1;
            if processed > 1_048_576 {
                return Err(Error::Malformed("Metal expanded method limit"));
            }
            let gpu::Method {
                subchannel,
                method,
                value,
                ..
            } = command;
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
            if class == 0xb197 && matches!(method, 0x114 | 0x118 | 0x11c | 0x120) {
                self.macros.write(method, value)?;
                continue;
            }
            if class == 0xb197 && (0x3800..=0x3ff8).contains(&method) && method % 8 == 0 {
                let mut params = Vec::new();
                while queue
                    .front()
                    .is_some_and(|m| m.subchannel == subchannel && m.method == method + 4)
                {
                    params.push(queue.pop_front().unwrap().value);
                    if params.len() > 256 {
                        return Err(Error::Malformed("Metal macro parameter limit"));
                    }
                }
                let generated = self
                    .macros
                    .run((method - 0x3800) / 8, value, &params, |reg| {
                        Ok(self.engine.registers.get(reg))
                    })?;
                if generated
                    .iter()
                    .any(|(reg, _)| (0x3800..=0x3ffc).contains(reg))
                {
                    return Err(Error::Malformed("Metal recursive macro"));
                }
                for (method, value) in generated.into_iter().rev() {
                    queue.push_front(gpu::Method {
                        method,
                        value,
                        ..command
                    });
                }
                continue;
            }
            if class == 0xb197 && method == 0x2c8 {
                if value & !0x110fff != 0 || value & 0xfff != syncpoint || value & (1 << 20) == 0 {
                    return Err(Error::Message(format!(
                        "Metal syncpoint operation {value:#x}, expected {syncpoint}"
                    )));
                }
                increments = increments
                    .checked_add(1)
                    .ok_or(Error::Malformed("Metal syncpoint overflow"))?;
                continue;
            }
            if class == 0xb197 && matches!(method, 0x15b4 | 0x1234 | 0x15d0 | 0x153c) {
                if value != 0 {
                    return Err(Error::Malformed(
                        "Metal CSAA, MSAA or linked TSC unsupported",
                    ));
                }
                self.engine.registers.0.insert(method, value);
                continue;
            }
            if class == 0xb197
                && ((0xd00..=0xd3c).contains(&method) && method % 4 == 0
                    || matches!(method, 0x194c | 0x1950))
            {
                if method == 0x194c && value > 1 || method == 0x1950 && value > 2 {
                    return Err(Error::Malformed("Metal window clip control"));
                }
                self.engine.registers.0.insert(method, value);
                continue;
            }
            if class == 0xb197 && matches!(method, 0x74c | 0x16a8 | 0x1794) {
                let valid = match method {
                    0x74c => value <= 255,
                    0x16a8 => value == 0x30003,
                    0x1794 => value == 0x20002,
                    _ => false,
                };
                if !valid {
                    return Err(Error::Malformed(
                        "Metal alpha fraction or interface version",
                    ));
                }
                self.engine.registers.0.insert(method, value);
                continue;
            }
            if class == 0x902d {
                let mask = match method {
                    0x260 | 0x290 | 0x29c | 0x2b8 | 0x888 => Some(1),
                    0x104 => Some(0x1ffffff),
                    0x108 => Some(u32::MAX),
                    0x884 => Some(0x3ff),
                    0x880 => Some(7),
                    0x88c => Some(0x11),
                    0x264 | 0x2a0 => Some(0xff),
                    0x26c | 0x294 => Some(7),
                    0x268 | 0x280..=0x28c | 0x298 | 0x2a4 | 0x2a8 => Some(u32::MAX),
                    _ => None,
                };
                if let Some(mask) = mask {
                    if method % 4 != 0 || value & !mask != 0 {
                        return Err(Error::Malformed("Metal 902D initialization field"));
                    }
                    self.twod.insert(method, value);
                    continue;
                }
                if method == 0x8dc
                    && (self.twod.get(&0x290).copied().unwrap_or(0) != 0
                        || self.twod.get(&0x29c).copied().unwrap_or(0) != 0
                        || self.twod.get(&0x2b8).copied().unwrap_or(0) != 0
                        || self.twod.get(&0x88c).copied().unwrap_or(0) & 0x10 != 0
                        || self.twod.get(&0x26c).copied().unwrap_or(1) != 1)
                {
                    return Err(Error::Malformed(
                        "Metal 902D clipping, key, zeta or conditional rendering unsupported",
                    ));
                }
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
                if snapshot.registers.get(0x194c) != 0 {
                    return Err(Error::Malformed(
                        "Metal enabled window clipping unsupported",
                    ));
                }
                backend.prepare(&snapshot, &memory).map_err(gpu_error)?;
                // Read back before any later transfer, upload, or texture read.
                let dir = self.directory()?;
                backend.execute(&dir, &mut memory).map_err(gpu_error)?;
            }
        }
        if expected_increments.is_some_and(|n| n != increments) {
            return Err(Error::Malformed(
                "Metal submission increment count mismatch",
            ));
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
        Ok((writes, increments))
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
