//! Synchronous Metal submission boundary. Guest writes and fences publish through the IPC transaction.
use super::*;
use soju_switch_gpu::{backend::Backend, engine::Engine, memory::Memory};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

mod compute;
mod copy;
mod programs;
mod setup;

struct MethodTrace {
    enabled: bool,
    class: u32,
    method: u32,
    value: u32,
    accepted: bool,
}
impl Drop for MethodTrace {
    fn drop(&mut self) {
        if self.enabled {
            eprintln!(
                "gpu-status|{:#06x}|{:#06x}|{:#010x}|{}",
                self.class,
                self.method,
                self.value,
                if self.accepted { "handled" } else { "rejected" }
            );
        }
    }
}

static NEXT: AtomicU64 = AtomicU64::new(0);
#[derive(Default)]
pub(super) struct Renderer {
    engine: Engine,
    backend: Backend,
    memory: Memory,
    mappings: Vec<(u64, u64, usize, u32, u64, u32)>,
    programs: programs::Cache,
    normalized_draw: Option<(
        soju_switch_gpu::engine::Registers,
        soju_switch_gpu::engine::Registers,
        bool,
    )>,
    copy: copy::CopyEngine,
    shadow_mode: u32,
    shadow: BTreeMap<u32, u32>,
    macros: gpu::mme::MacroUpload,
    bindings: BTreeMap<u32, u32>,
    twod: BTreeMap<u32, u32>,
    compute_setup: BTreeMap<u32, u32>,
    pub(super) noop_counts: BTreeMap<(u32, u32), u64>,
    compute_counters: BTreeMap<u32, u32>,
    compute_upload: Engine,
    compute: soju_switch_gpu::compute::Engine,
    graphics_upload: Engine,
    directory: Option<PathBuf>,
    faulted: bool,
}
fn gpu_error(e: soju_switch_gpu::Error) -> Error {
    Error::Message(format!("Metal submission: {e}"))
}
// These registers launch actions or select uploads already represented by Action/constants.
fn draw_bookkeeping(offset: u32) -> bool {
    matches!(
        offset,
        0xd74 | 0xd78 | 0x1614 | 0x1618 | 0x19d0 | 0x2380
            ..=0x23cc | 0x2410 | 0x2430 | 0x2450 | 0x2470 | 0x2490
    )
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
        let snapshot_span =
            soju_switch_gpu::profile::span(soju_switch_gpu::profile::Stage::Snapshot);
        let mappings = access
            .space
            .mappings
            .iter()
            .map(|(&a, m)| {
                let size =
                    usize::try_from(m.size).map_err(|_| Error::Malformed("Metal mapping size"))?;
                let (guest, _) = access.space.resolve(a, size)?;
                Ok((a, guest, size, m.view.allocation, m.view.generation, m.kind))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let reset = mappings != self.mappings;
        let mut memory = if reset {
            self.programs = programs::Cache::default();
            Memory::default()
        } else {
            std::mem::take(&mut self.memory)
        };
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
            if let Some(slices) = access.memory.segments(guest, size)? {
                if reset {
                    let mut bytes = Vec::with_capacity(size);
                    for slice in slices {
                        bytes.extend_from_slice(slice);
                    }
                    memory.map(address, bytes).map_err(gpu_error)?;
                } else {
                    let mut offset = 0;
                    for slice in slices {
                        memory
                            .synchronize(address + offset, slice)
                            .map_err(gpu_error)?;
                        offset += slice.len() as u64;
                    }
                }
            } else {
                let mut bytes = vec![0; size];
                access.memory.read(guest, &mut bytes)?;
                if reset {
                    memory.map(address, bytes).map_err(gpu_error)?;
                } else {
                    memory.synchronize(address, &bytes).map_err(gpu_error)?;
                }
            }
            original.push((address, guest, size));
        }
        let captured_revision = memory.revision();
        drop(snapshot_span);
        let mut backend = std::mem::take(&mut self.backend);
        let mut pending = false;
        let mut target_state = Vec::new();
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
                mut value,
                ..
            } = command;
            let mut trace = MethodTrace {
                enabled: std::env::var_os("SOJU_SWITCH_TRACE_GPU").is_some(),
                class: if method == 0 {
                    value
                } else {
                    self.bindings.get(&subchannel).copied().unwrap_or(0)
                },
                method,
                value,
                accepted: false,
            };
            if method == 0 {
                if !matches!(value, 0xb197 | 0xa140 | 0x902d | 0xb0b5 | 0xb1c0) {
                    return Err(Error::Message(format!(
                        "Metal unsupported class {value:#x}"
                    )));
                }
                if value == 0xb197 {
                    // The runner's GM20B graphics context uses Maxwell TICs.
                    self.engine.registers.0.entry(0xf10).or_insert(1);
                }
                self.bindings.insert(subchannel, value);
                trace.accepted = true;
                continue;
            }
            // A06F L2_FLUSH_DIRTY: every prior action has synchronously read back.
            if method == 0x2c && matches!(value, 0x80000000 | 0x70000000) {
                if pending {
                    backend
                        .execute_memory(&self.directory()?, &mut memory)
                        .map_err(gpu_error)?;
                    pending = false;
                    target_state.clear();
                }
                trace.accepted = true;
                continue;
            }
            let class = *self
                .bindings
                .get(&subchannel)
                .ok_or(Error::Malformed("Metal unbound subchannel"))?;
            // CPU engines and visibility operations must observe all preceding draws.
            // Constant uploads may remain batched when they do not overlap outputs;
            // prepare already owns the constants used by preceding draws.
            let constant_write = class == 0xb197 && (0x2390..=0x23cc).contains(&method);
            let constant_hazard = constant_write
                && self
                    .engine
                    .registers
                    .address(0x2384)
                    .checked_add(u64::from(self.engine.registers.get(0x238c)))
                    .is_none_or(|address| backend.pending_overlap(address, 4));
            if pending
                && (class != 0xb197
                    || constant_hazard
                    || matches!(method, 0x180..=0x1b4 | 0x1b0c | 0x2c8 | 0x2e0 | 0xf80))
            {
                backend
                    .execute_memory(&self.directory()?, &mut memory)
                    .map_err(gpu_error)?;
                pending = false;
                target_state.clear();
            }
            if class == 0xb197 && method == 0x124 {
                if value > 3 {
                    return Err(Error::Malformed("MME shadow mode"));
                }
                self.shadow_mode = value;
                trace.accepted = true;
                continue;
            }
            if class == 0xb197 && setup::shadowed(method) {
                match self.shadow_mode {
                    0 | 1 => {
                        self.shadow.insert(method, value);
                    }
                    3 => {
                        value = *self
                            .shadow
                            .get(&method)
                            .ok_or(Error::Malformed("uninitialized MME shadow register"))?
                    }
                    _ => {}
                }
            } else if class == 0xb197 && self.shadow_mode == 3 {
                return Err(Error::Message(format!(
                    "unsupported MME shadow replay method {method:#x}"
                )));
            }
            if class == 0xb197 && matches!(method, 0x114 | 0x118 | 0x11c | 0x120) {
                self.macros.write(method, value)?;
                trace.accepted = true;
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
                let macro_span =
                    soju_switch_gpu::profile::span(soju_switch_gpu::profile::Stage::Macro);
                let generated = self
                    .macros
                    .run((method - 0x3800) / 8, value, &params, |reg| {
                        Ok(self.engine.registers.get(reg))
                    })?;
                drop(macro_span);
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
                trace.accepted = true;
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
                trace.accepted = true;
                continue;
            }
            if class == 0xb197
                && matches!(
                    method,
                    0x180 | 0x184 | 0x188 | 0x18c | 0x190 | 0x1b0 | 0x1b4
                )
            {
                let upload_value = if method == 0x1b0 {
                    if value & !0x1050 != 1 {
                        return Err(Error::Malformed("B197 inline launch flags"));
                    }
                    1
                } else {
                    value
                };
                self.graphics_upload
                    .method(0xa140, method, upload_value, &mut memory)
                    .map_err(gpu_error)?;
                trace.accepted = true;
                continue;
            }
            if class == 0xa140 && method == 0x1b0 && value & !0x1050 == 1 {
                value = 1;
            }
            if class == 0xb0b5 {
                self.copy.method(method, value, &mut memory)?;
                trace.accepted = true;
                continue;
            }
            if class == 0xb1c0 {
                if matches!(method, 0x1608 | 0x160c | 0x2b4 | 0x2bc) {
                    if let Some(dispatch) = self
                        .compute
                        .method(class, method, value, &memory)
                        .map_err(gpu_error)?
                    {
                        compute::register(&dispatch, &memory, &original, &mut backend)?;
                        backend
                            .prepare_compute(&dispatch, &memory)
                            .map_err(gpu_error)?;
                        let dir = self.directory()?;
                        backend
                            .execute_memory(&dir, &mut memory)
                            .map_err(gpu_error)?;
                    }
                } else if setup::compute_field(method, value)? {
                    self.compute_setup.insert(method, value);
                } else if method == 0x248 {
                    if value & !0xffff3f != 0 {
                        return Err(Error::Malformed("Metal compute reference counter fields"));
                    }
                    self.compute_counters.insert(value & 63, value >> 8);
                } else if matches!(
                    method,
                    0x180 | 0x184 | 0x188 | 0x18c | 0x190 | 0x1b0 | 0x1b4
                ) {
                    let value = if method == 0x1b0 {
                        if value & !0x1050 != 1 {
                            return Err(Error::Malformed("Metal compute inline launch flags"));
                        }
                        1
                    } else {
                        value
                    };
                    self.compute_upload
                        .method(0xa140, method, value, &mut memory)
                        .map_err(gpu_error)?;
                } else {
                    return Err(Error::Message(format!(
                        "Metal compute method {method:#x}={value:#x} unsupported"
                    )));
                }
                trace.accepted = true;
                continue;
            }
            if class == 0xb197 && method == 0x1b0c && value == 0x1000f010 {
                let address = self.engine.registers.address(0x1b00);
                if address & 3 != 0 {
                    return Err(Error::Malformed("Metal release semaphore alignment"));
                }
                memory
                    .write(address, &self.engine.registers.get(0x1b08).to_le_bytes())
                    .map_err(gpu_error)?;
                trace.accepted = true;
                continue;
            }
            // Compression is not represented in guest storage; all preceding work is complete.
            if class == 0xb197 && matches!(method, 0x2e0 | 0xf80) && value == 0 {
                trace.accepted = true;
                continue;
            }
            if setup::accepted_noop(class, method) {
                let count = self.noop_counts.entry((class, method)).or_default();
                *count = count.saturating_add(1);
                eprintln!("gpu-noop|{class:#06x}|{method:#06x}|{value:#010x}|count={count}");
                trace.accepted = true;
                continue;
            }
            if class == 0xb197 && setup::retain(method, value)? {
                self.engine.registers.0.insert(method, value);
                trace.accepted = true;
                continue;
            }
            if class == 0xb197 && matches!(method, 0x15b4 | 0x1234 | 0x15d0 | 0x153c) {
                if value != 0 {
                    return Err(Error::Malformed(
                        "Metal CSAA, MSAA or linked TSC unsupported",
                    ));
                }
                self.engine.registers.0.insert(method, value);
                trace.accepted = true;
                continue;
            }
            if class == 0xb197 && matches!(method, 0x165c | 0xde4) {
                // clb197: POINT_CENTER_MODE (OGL/D3D), SM_TIMEOUT_INTERVAL.COUNTER_BIT.
                // The host scheduler owns execution timeouts; retain the guest setting.
                if (method == 0x165c && value > 1) || (method == 0xde4 && value > 63) {
                    return Err(Error::Malformed("Metal point center or SM timeout field"));
                }
                self.engine.registers.0.insert(method, value);
                trace.accepted = true;
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
                trace.accepted = true;
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
                trace.accepted = true;
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
                    trace.accepted = true;
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
            if let Some(mut snapshot) = snapshot {
                if matches!(
                    snapshot.action,
                    soju_switch_gpu::engine::Action::Draw { topology: 0, .. }
                ) {
                    return Err(Error::Malformed(
                        "Metal point center rasterization unverified",
                    ));
                }
                if snapshot.registers.get(0x194c) != 0 {
                    return Err(Error::Malformed(
                        "Metal enabled window clipping unsupported",
                    ));
                }
                if let soju_switch_gpu::engine::Action::Clear(mask) = snapshot.action {
                    let r = &mut snapshot.registers;
                    if mask & 3 == 0 {
                        r.0.insert(0x1538, 0);
                    }
                    let control = r.get(0x10f8);
                    if control & !0x101 != 0 {
                        return Err(Error::Malformed(
                            "core clear rectangle/viewport control unsupported",
                        ));
                    }
                    if control & 0x100 == 0
                        || (r.get(0xe04) == r.get(0x808) << 16
                            && r.get(0xe08) == r.get(0x80c) << 16)
                    {
                        r.0.insert(0xe00, 0);
                    }
                    // A clear does not consume shader, blend, vertex or raster setup.
                    r.0.retain(|offset, _| {
                        matches!(*offset,
                        0x800..=0x9e0 | 0xd80..=0xd90 | 0xda0 | 0xe00..=0xe08 | 0x121c
                            | 0xfe0..=0xff0 | 0x1228..=0x1230 | 0x1538)
                    });
                }
                if matches!(
                    snapshot.action,
                    soju_switch_gpu::engine::Action::Draw { .. }
                ) {
                    let indexed = matches!(
                        snapshot.action,
                        soju_switch_gpu::engine::Action::Draw { indexed: true, .. }
                    );
                    if pending && backend.reads_pending(&snapshot) {
                        backend
                            .execute_memory(&self.directory()?, &mut memory)
                            .map_err(gpu_error)?;
                        pending = false;
                        target_state.clear();
                    }
                    programs::register(
                        &snapshot,
                        &memory,
                        &original,
                        &mut backend,
                        &mut self.programs,
                    )?;
                    let cached = self
                        .normalized_draw
                        .as_ref()
                        .filter(|(source, _, old_indexed)| {
                            *old_indexed == indexed
                                && source.0.iter().eq(snapshot
                                    .registers
                                    .0
                                    .iter()
                                    .filter(|(offset, _)| !draw_bookkeeping(**offset)))
                        })
                        .map(|(_, normalized, _)| normalized.clone());
                    if let Some(normalized) = cached {
                        snapshot.registers = normalized;
                    } else {
                        let mut source = snapshot.registers.clone();
                        source.0.retain(|offset, _| !draw_bookkeeping(*offset));
                        let r = &mut snapshot.registers;
                        if matches!(
                            snapshot.action,
                            soju_switch_gpu::engine::Action::Draw { indexed: false, .. }
                        ) && r.get(0xde8) != 0
                            && r.get(0x1644) != 0
                        {
                            return Err(Error::Malformed(
                                "nonindexed primitive restart unsupported",
                            ));
                        }
                        if r.get(0xf90) != 0 && r.get(0x1a00) != 0x1111 {
                            return Err(Error::Malformed(
                                "partial shared color write mask unsupported",
                            ));
                        }
                        for index in 0..16 {
                            let clip_base = 0xc00 + index * 16;
                            let scissor_base = 0xe00 + index * 16;
                            for axis in [0, 4] {
                                let clip = clip_base + axis;
                                let scissor = scissor_base + 4 + axis;
                                let lo = r.get(clip) & 0xffff;
                                let hi = lo + (r.get(clip) >> 16);
                                if hi > 65535 {
                                    return Err(Error::Malformed("viewport clip overflow"));
                                }
                                let (lo, hi) = if r.get(scissor_base) == 0 {
                                    (lo, hi)
                                } else {
                                    let start = lo.max(r.get(scissor) & 0xffff);
                                    (start, hi.min(r.get(scissor) >> 16).max(start))
                                };
                                r.0.insert(scissor, lo | (hi << 16));
                            }
                            r.0.insert(scissor_base, 1);
                            if r.get(clip_base + 8) != 0
                                || (r.0.contains_key(&(clip_base + 12))
                                    && r.get(clip_base + 12) != 1f32.to_bits())
                            {
                                return Err(Error::Malformed(
                                    "non-default viewport depth clip unsupported",
                                ));
                            }
                        }
                        if snapshot.registers.get(0x12cc) == 0
                            && snapshot.registers.get(0x1380) == 0
                        {
                            snapshot.registers.0.insert(0x1538, 0);
                        }
                        // Hardware Y is scale * NDC + offset; Metal's viewport reverses NDC Y.
                        for index in 0..16 {
                            let offset = 0xa04 + index * 32;
                            if let Some(value) = snapshot.registers.0.get_mut(&offset) {
                                *value = (-f32::from_bits(*value)).to_bits();
                            }
                        }
                        snapshot.registers.0.retain(|&offset, value| {
                            (matches!(offset, 0x1684 | 0x113c | 0x12ec | 0x1310 | 0x1314)
                                || (0xa00..=0xbf4).contains(&offset) && offset % 32 <= 20
                                || (0xe00..=0xef8).contains(&offset) && offset % 16 <= 8
                                || !setup::retain(offset, *value).unwrap_or(false))
                                && !matches!(offset, 0x15b4 | 0x1234 | 0x15d0 | 0x153c
                                | 0x165c | 0xde4 | 0xd00..=0xd3c | 0x194c | 0x1950
                                | 0x74c | 0x16a8 | 0x1794 | 0xc00..=0xc0c)
                        });
                        snapshot
                            .registers
                            .0
                            .retain(|offset, _| !draw_bookkeeping(*offset));
                        self.normalized_draw = Some((source, snapshot.registers.clone(), indexed));
                    }
                }
                let r = &snapshot.registers;
                let targets: Vec<_> = (0x800..=0x9e0)
                    .step_by(4)
                    .chain([
                        0x121c, 0xfe0, 0xfe4, 0xfe8, 0xfec, 0x1538, 0x1228, 0x122c, 0x1230,
                    ])
                    .map(|offset| r.get(offset))
                    .collect();
                if pending && (targets != target_state || backend.reads_pending(&snapshot)) {
                    backend
                        .execute_memory(&self.directory()?, &mut memory)
                        .map_err(gpu_error)?;
                }
                backend.prepare(&snapshot, &memory).map_err(gpu_error)?;
                pending = true;
                target_state = targets;
            }
            trace.accepted = true;
        }
        if expected_increments.is_some_and(|n| n != increments) {
            return Err(Error::Malformed(
                "Metal submission increment count mismatch",
            ));
        }
        if pending {
            backend
                .execute_memory(&self.directory()?, &mut memory)
                .map_err(gpu_error)?;
        }
        self.backend = backend;
        let _commit_span = soju_switch_gpu::profile::span(soju_switch_gpu::profile::Stage::Commit);
        let mut writes = Vec::new();
        for (address, guest, size) in original {
            for offset in (0..size).step_by(4096) {
                let n = (size - offset).min(4096);
                let source = address + offset as u64;
                if memory.generation(source, n).map_err(gpu_error)? <= captured_revision {
                    continue;
                }
                let after = memory.read(source, n).map_err(gpu_error)?;
                let destination = guest + offset as u64;
                let mut scratch = [0; 4096];
                let before = if let Some(bytes) = access.memory.contiguous(destination, n)? {
                    bytes
                } else {
                    access.memory.read(destination, &mut scratch[..n])?;
                    &scratch[..n]
                };
                if before != after {
                    access.memory.check_access(destination, n, 3)?;
                    super::commit::validate_target(destination, n, protected)?;
                    writes.push(MemoryWrite {
                        address: destination,
                        bytes: after.to_vec(),
                    });
                }
            }
        }
        self.memory = memory;
        self.mappings = mappings;
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
