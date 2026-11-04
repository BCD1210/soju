//! Resource resolution and a dependency-free protocol to the native Metal worker.
use crate::{
    Error, Result,
    engine::{Action, ConstantBuffer, Registers, Snapshot},
    memory::Memory,
    require,
    texture::{TextureCache, Tic, Tsc},
};
use soju_switch_shader::{
    cache::{Inputs, Key},
    header::{Header, Stage},
    msl::{MetalShader, Options},
};
use soju_switch_texture::Surface;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

mod compute;
mod fill_rectangle;
use crate::protocol::*;
use std::fmt::Write;

#[derive(Default)]
pub struct Backend {
    compute_programs: BTreeMap<u64, (usize, Options)>,
    compute_targets: BTreeMap<String, compute::ComputeTarget>,
    programs: BTreeMap<u64, (usize, Options)>,
    pub textures: TextureCache,
    commands: Vec<Command>,
    profile: Option<[std::time::Duration; 3]>,
    variants: Vec<PreparedVariant>,
    prepared: Option<(Snapshot, (u64, u64), usize)>,
    file_cache: std::collections::HashMap<Vec<u8>, String>,
    attachment_files: std::collections::HashMap<(String, u64), (String, String)>,
    pipeline_states: crate::lru::Lru<Vec<u8>, (MetalShader, MetalShader, String)>,
    shader_cache: crate::lru::Lru<(u64, u64, u64), (MetalShader, Header, Key)>,
    resource_files: std::collections::HashMap<(u64, u64, usize, u64), String>,
    files: Vec<crate::upload::Upload>,
    constant_upload: Vec<u8>,
    uniform_offsets: std::collections::HashMap<(u64, u64, u32), (u64, usize)>,
    output_generations: std::collections::HashMap<String, (u64, u64, Vec<crate::upload::Upload>)>,
    targets: BTreeMap<String, Target>,
    depth_targets: BTreeMap<String, Target>,
    worker: Option<crate::worker::Worker>,
    serial: u64,
    queries: BTreeMap<String, (u64, u64, u64)>,
}
struct PreparedVariant {
    state: Snapshot,
    index: usize,
    epoch: (u64, u64),
    dependencies: Vec<(u64, usize, u64)>,
    constants: Vec<ConstantBuffer>,
    views: (usize, usize),
    vertex_capacity: u64,
}
fn compatible_draw(previous: &Snapshot, r: &Registers, action: &Action, capacity: u64) -> bool {
    match (&previous.action, action) {
        (
            Action::Draw {
                topology: old_topology,
                indexed: false,
                instance: old_instance,
                ..
            },
            Action::Draw {
                topology,
                first,
                count,
                indexed: false,
                instance,
            },
        ) => {
            r.get(0x113c) == 0
                && *topology != 7
                && topology == old_topology
                && instance == old_instance
                && (1..=1_000_000).contains(count)
                && first
                    .checked_add(*count - 1)
                    .is_some_and(|last| u64::from(last) < capacity)
                && previous
                    .registers
                    .0
                    .iter()
                    .filter(|(a, _)| !matches!(**a, 0xd74 | 0xd78))
                    .eq(r.0.iter().filter(|(a, _)| !matches!(**a, 0xd74 | 0xd78)))
        }
        _ => false,
    }
}
fn vertex_capacity(command: &Command, files: &[crate::upload::Upload]) -> u64 {
    command
        .attributes
        .iter()
        .filter(|a| a.instanced == 0)
        .map(|a| {
            let Some(buffer) = command.buffers.iter().find(|b| b.slot == a.stream) else {
                return 0;
            };
            let Some(index) = std::str::from_utf8(&buffer.file)
                .ok()
                .and_then(|s| s.strip_prefix("resource-"))
                .and_then(|s| s.strip_suffix(".bin"))
                .and_then(|s| s.parse::<usize>().ok())
            else {
                return 0;
            };
            let Some(bytes) = files.get(index) else {
                return 0;
            };
            let width = match a.format {
                31 => 16,
                30 => 12,
                29 => 8,
                28 | 9 => 4,
                _ => return 0,
            };
            if a.stride <= 0 || a.offset < 0 || bytes.len() < a.offset as usize + width {
                return 0;
            }
            ((bytes.len() - a.offset as usize - width) / a.stride as usize + 1) as u64
        })
        .min()
        .unwrap_or(u64::from(u32::MAX) + 1)
}
fn capture<'a>(
    memory: &'a Memory,
    dependencies: &mut Vec<(u64, usize, u64)>,
    address: u64,
    size: usize,
) -> Result<&'a [u8]> {
    dependencies.push((address, size, memory.generation(address, size)?));
    memory.read(address, size)
}
#[derive(Clone)]
struct Target {
    address: u64,
    width: u32,
    height: u32,
    block_height: u32,
    pitch: bool,
    generation: u64,
    native_generation: u64,
    memory_identity: u64,
    format: soju_switch_texture::format::Format,
    // Rotate guest packed depth into the canonical Z24S8 order on import.
    depth_rotation: u32,
}
#[derive(Debug)]
pub struct Readback {
    pub address: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
#[derive(Debug)]
pub struct Report {
    pub images: Vec<Readback>,
    pub diagnostics: String,
}
fn io(e: impl std::fmt::Display) -> Error {
    Error(e.to_string())
}
fn uniform_offset(
    upload: &mut Vec<u8>,
    offsets: &mut std::collections::HashMap<(u64, u64, u32), (u64, usize)>,
    cb: ConstantBuffer,
    memory: &Memory,
) -> Result<usize> {
    let generation = memory.generation(cb.address, cb.size as usize)?;
    let key = (memory.identity, cb.address, cb.size);
    if let Some(&(previous, offset)) = offsets.get(&key) {
        if previous == generation {
            return Ok(offset);
        }
    }
    let offset = upload.len().div_ceil(256) * 256;
    let bytes = memory.read(cb.address, cb.size as usize)?;
    if upload.capacity() < offset + bytes.len() {
        upload.reserve((offset + bytes.len()).next_power_of_two().max(1024 * 1024) - upload.len());
    }
    upload.resize(offset, 0);
    upload.extend_from_slice(bytes);
    offsets.insert(key, (generation, offset));
    Ok(offset)
}
impl Target {
    fn surface(&self) -> Surface {
        Surface {
            width: self.width as usize,
            height: self.height as usize,
            depth: 1,
            bytes_per_block: self.format.info().bytes,
            block_height_log2: self.block_height,
            block_depth_log2: 0,
        }
    }
    fn writeback(&self, bytes: &[u8], memory: &Memory) -> Result<Vec<u8>> {
        require(
            bytes.len() == self.width as usize * self.height as usize * self.format.info().bytes,
            "invalid attachment readback size",
        )?;
        if self.pitch {
            return Ok(bytes.to_vec());
        }
        let surface = self.surface();
        let mut data = memory.read(self.address, self.size())?.to_vec();
        let row = self.width as usize * self.format.info().bytes;
        for y in 0..self.height as usize {
            for x in (0..row).step_by(16) {
                let n = (row - x).min(16);
                let offset = surface.offset(x, y, 0);
                data[offset..offset + n].copy_from_slice(&bytes[y * row + x..y * row + x + n]);
            }
        }
        Ok(data)
    }
    fn size(&self) -> usize {
        if self.pitch {
            self.width as usize * self.height as usize * self.format.info().bytes
        } else {
            self.surface().size()
        }
    }
}
impl Backend {
    /// Whether a CPU access intersects output still owned by this batch.
    pub fn pending_overlap(&self, address: u64, size: u64) -> bool {
        let overlaps = |a: u64, n: u64| {
            u128::from(address) < u128::from(a) + u128::from(n)
                && u128::from(a) < u128::from(address) + u128::from(size)
        };
        self.targets
            .values()
            .chain(self.depth_targets.values())
            .any(|t| overlaps(t.address, t.size() as u64))
            || self.queries.values().any(|&(a, _, _)| overlaps(a, 4))
            || self
                .compute_targets
                .values()
                .any(|t| overlaps(t.address, t.size as u64))
    }
    /// Conservative input hazard check. Textured draws use a visibility boundary
    /// until texture descriptors and their backing ranges are tracked here.
    pub fn reads_pending(&self, s: &Snapshot) -> bool {
        let r = &s.registers;
        if s.constants
            .iter()
            .flatten()
            .flatten()
            .any(|cb| self.pending_overlap(cb.address, u64::from(cb.size)))
        {
            return true;
        }
        for offset in [0x2044, 0x2144] {
            let Some(address) = r.address(0x1608).checked_add(u64::from(r.get(offset))) else {
                return true;
            };
            let Some((_, options)) = self.programs.get(&address) else {
                return true;
            };
            if self.pending_overlap(address, 1024 * 1024) || !options.texture_bindings.is_empty() {
                return true;
            }
        }
        for (start, end) in std::iter::once((r.address(0x17c8), r.address(0x17d0)))
            .chain((0..16).map(|i| (r.address(0x1c04 + i * 16), r.address(0x1f00 + i * 8))))
        {
            if end >= start
                && self.pending_overlap(start, end.saturating_sub(start).saturating_add(1))
            {
                return true;
            }
        }
        false
    }
    /// Opt-in phase timing: attachment/state validation, shader/key construction,
    /// and remaining resource capture. Cached repeats bypass all three.
    pub fn enable_profiling(&mut self) {
        self.profile = Some([std::time::Duration::ZERO; 3]);
    }
    pub fn preparation_profile(&self) -> Option<[std::time::Duration; 3]> {
        self.profile
    }
    fn checkpoint(&mut self, clock: &mut Option<std::time::Instant>, phase: usize) {
        if let (Some(start), Some(totals)) = (clock.as_mut(), self.profile.as_mut()) {
            totals[phase] += start.elapsed();
            *start = std::time::Instant::now();
        }
    }

    /// Program extents come from the runner's shader allocation tracker. No guessed
    /// instruction scan beyond an allocation; bytes are reread at every launch.
    pub fn program(&mut self, address: u64, size: usize, options: Options) -> Result<()> {
        require(
            (80..=1024 * 1024).contains(&size),
            "invalid shader allocation length",
        )?;
        require(
            options.texture_count <= 16
                && options.texture_buffer == 30
                && options.global_memory.is_none(),
            "unsupported resource ABI",
        )?;
        if self
            .programs
            .get(&address)
            .is_some_and(|(previous_size, previous)| *previous_size == size && *previous == options)
        {
            return Ok(());
        }
        self.variants.clear();
        self.shader_cache.clear();
        self.prepared = None;
        self.programs.insert(address, (size, options));
        Ok(())
    }
    fn file(&mut self, bytes: Vec<u8>) -> String {
        if let Some(name) = self.file_cache.get(&bytes) {
            return name.clone();
        }
        let name = format!("resource-{}.bin", self.files.len());
        self.file_cache.insert(bytes.clone(), name.clone());
        self.files.push(crate::upload::Upload::copy(&bytes));
        name
    }
    fn retained_attachments(
        &mut self,
        key: &str,
        generation: u64,
        count: usize,
    ) -> Option<Vec<String>> {
        let (_, _, files) = self
            .output_generations
            .get(key)
            .filter(|(g, _, files)| *g == generation && files.len() == count)?;
        let files = files.clone();
        Some(
            files
                .into_iter()
                .map(|file| {
                    let name = format!("resource-{}.bin", self.files.len());
                    self.files.push(file);
                    name
                })
                .collect(),
        )
    }
    fn resource_file(
        &mut self,
        memory: &Memory,
        dependencies: &mut Vec<(u64, usize, u64)>,
        address: u64,
        length: usize,
    ) -> Result<String> {
        let generation = memory.generation(address, length)?;
        dependencies.push((address, length, generation));
        let key = (memory.identity, address, length, generation);
        if let Some(file) = self.resource_files.get(&key) {
            return Ok(file.clone());
        }
        // Address/generation identity already proves reuse; no full-buffer content hash.
        let name = format!("resource-{}.bin", self.files.len());
        self.files
            .push(crate::upload::Upload::copy(memory.read(address, length)?));
        self.resource_files.insert(key, name.clone());
        Ok(name)
    }
    fn shader(
        &mut self,
        address: u64,
        memory: &Memory,
        stage: Stage,
    ) -> Result<(MetalShader, Header, Key)> {
        let (size, options) = self
            .programs
            .get(&address)
            .ok_or_else(|| Error(format!("shader extent not registered: {address:#x}")))?;
        let identity = (memory.identity, address, memory.generation(address, *size)?);
        if let Some(cached) = self.shader_cache.get(&identity) {
            require(cached.1.stage == stage, "pipeline stage/SPH mismatch")?;
            return Ok(cached.clone());
        }
        let bytes = memory.read(address, *size)?;
        let header = Header::parse(bytes).map_err(io)?;
        require(header.stage == stage, "pipeline stage/SPH mismatch")?;
        let translation = crate::profile::span(crate::profile::Stage::ShaderTranslation);
        let shader = soju_switch_shader::translate(bytes, options).map_err(io)?;
        drop(translation);
        // MSL includes normalized resource ABI and all compiler-facing translation inputs.
        let key = Key::new(Inputs {
            program: bytes,
            header: &[],
            render_state: shader.source.as_bytes(),
        });
        self.shader_cache.insert(
            identity,
            (shader.clone(), header.clone(), key),
            shader.source.len() + 512,
        );
        Ok((shader, header, key))
    }
    /// Resolve immediately at each action, before later CPU/transfer writes.
    pub fn prepare(&mut self, s: &Snapshot, memory: &Memory) -> Result<()> {
        let _span = crate::profile::span(crate::profile::Stage::Prepare);
        self.prepare_state(&s.registers, &s.constants, &s.action, memory)
    }
    pub(crate) fn prepare_state(
        &mut self,
        r: &Registers,
        bindings: &[[Option<ConstantBuffer>; 32]; 5],
        action: &Action,
        memory: &Memory,
    ) -> Result<()> {
        require(
            self.compute_targets.is_empty(),
            "readback compute before graphics",
        )?;
        if let Some((previous, epoch, index)) = &self.prepared {
            if *epoch == memory.epoch()
                && previous.registers == *r
                && previous.constants == *bindings
                && previous.action == *action
            {
                self.commands[*index].repeats += 1;
                return Ok(());
            }
        }
        if let Some(variant) = self.variants.iter_mut().rev().find(|v| {
            v.epoch.0 == memory.identity
                && v.state.constants == *bindings
                && ((v.state.registers == *r && v.state.action == *action)
                    || compatible_draw(&v.state, r, action, v.vertex_capacity))
                && v.views == (self.targets.len(), self.depth_targets.len())
                && (v.epoch == memory.epoch()
                    || v.dependencies
                        .iter()
                        .all(|&(a, n, g)| memory.generation(a, n) == Ok(g)))
        }) {
            let _reference = crate::profile::span(crate::profile::Stage::DrawReference);
            let template = &self.commands[variant.index];
            let mut command = Command {
                kind: 5,
                reference: variant.index as _,
                repeats: 1,
                ..Command::default()
            };
            if variant.state.action != *action {
                if let Action::Draw { first, count, .. } = action {
                    command.kind = 6;
                    command.first = i64::from(*first);
                    command.count = i64::from(*count);
                }
            }
            for (cb, binding) in variant.constants.iter().zip(template.constants.iter()) {
                let offset = uniform_offset(
                    &mut self.constant_upload,
                    &mut self.uniform_offsets,
                    *cb,
                    memory,
                )?;
                let offsets = if binding.stage == 0 {
                    &mut command.constant_vertex
                } else {
                    &mut command.constant_fragment
                };
                offsets[binding.slot as usize] = offset as _;
            }
            self.commands.push(command);
            variant.epoch = memory.epoch();
            self.prepared = None;
            return Ok(());
        }
        let mut dependencies = Vec::new();
        let mut captured_constants = Vec::new();
        self.prepared = None;
        let mut clock = self.profile.map(|_| std::time::Instant::now());
        match *action {
            Action::ResetOcclusion => {
                self.commands.push(Command {
                    kind: 2,
                    ..Command::default()
                });
                return Ok(());
            }
            Action::ReportOcclusion { address } => {
                let generation = memory.generation(address, 4)?;
                let key = format!("{}-{address:x}", memory.identity);
                self.commands.push(Command {
                    kind: 3,
                    key: key.clone().into(),
                    ..Command::default()
                });
                self.queries
                    .insert(key, (address, generation, memory.identity));
                return Ok(());
            }
            _ => {}
        }
        for &offset in r.0.keys() {
            require(
                !crate::engine::stream_output_method(offset),
                "transform feedback (stream output) is not implemented",
            )?;
            let supported = matches!(offset,0x15d0|0x1534|0x11e0..=0x11ec|0x1684|0x113c|0x12ec|0x1310|0x1314|0x800..=0x9e0|0xa00..=0xa14|0xd74..=0xd90|0xda0|0xdac|0xdb0|0xdc0..=0xdc8|0x1110|0x1510..=0x1518|0x1550..=0x1558|0x156c|0x15bc|0x1644|0x1648|0x187c|0x1910|0x1940|0xe00..=0xe08|0xf10|0xf54..=0xf5c|0xfe0..=0xff0|0x1160..=0x11dc|0x121c|0x1228..=0x1230|0x12cc|0x12e4|0x12e8|0x130c|0x131c..=0x1328|0x133c..=0x139c|0x1434|0x1438|0x1530|0x1538|0x1b00..=0x1b0c|0x155c..=0x1564|0x1574..=0x157c|0x1594..=0x15a4|0x1608|0x160c|0x1614|0x1618|0x17c8..=0x17e0|0x1880..=0x18fc|0x1918..=0x1920|0x19d0|0x1c00..=0x1eff|0x1f00..=0x1fff|0x2000..=0x2150|0x2380..=0x23cc|0x2410|0x2430|0x2450|0x2470|0x2490);
            let viewport_array = (0xa00..=0xbf4).contains(&offset) && offset % 32 <= 20;
            let scissor_array = (0xe00..=0xef8).contains(&offset) && offset % 16 <= 8;
            if !supported && !viewport_array && !scissor_array {
                return Err(Error(format!(
                    "Metal state method {offset:#x} is not implemented"
                )));
            }
        }
        if let Some(&mode) = r.0.get(&0x1558) {
            require(
                mode <= 1,
                "memory conditional rendering mode not implemented",
            )?;
            if mode == 0 {
                return Ok(());
            }
        }
        let samples = match r.get(0x15d0) {
            0 => 1,
            2 => 4,
            _ => return Err(Error("unsupported multisample storage mode".into())),
        };
        require(r.get(0x1534) <= 1, "invalid multisample enable")?;
        let scale = if samples == 4 { 2 } else { 1 };
        let attachment_count = r.get(0x121c) & 15;
        require(
            (1..=8).contains(&attachment_count) && r.get(0x121c) >> 28 == 0,
            "invalid CT_SELECT count",
        )?;
        let mut selected = BTreeSet::new();
        let mut batch_targets = BTreeMap::<String, Target>::new();
        let mut target_commands = Vec::new();
        for slot in 0..attachment_count {
            let index = (r.get(0x121c) >> (4 + slot * 3)) & 7;
            require(selected.insert(index), "duplicate CT_SELECT target")?;
            let base = 0x800 + index * 64;
            let format = crate::format::color(r.get(base + 0x10))?;
            let bytes_per_pixel = format.info().bytes as u32;
            require(
                r.get(base + 0x18) == 1 && r.get(base + 0x20) == 0,
                "layered render target unsupported",
            )?;
            let layout = r.get(base + 0x14);
            require(
                layout & !0x1070 == 0 && (layout >> 4) & 7 <= 5,
                "unsupported target memory layout",
            )?;
            let pitch = layout & 0x1000 != 0;
            let width = if pitch {
                require(
                    r.get(base + 0x08).is_multiple_of(bytes_per_pixel),
                    "RGBA target pitch alignment",
                )?;
                r.get(base + 0x08) / bytes_per_pixel
            } else {
                r.get(base + 0x08)
            };
            let height = r.get(base + 0x0c);
            require(
                width > 0 && height > 0 && width <= 16384 && height <= 16384,
                "invalid render target size",
            )?;
            require(width.is_multiple_of(scale) && height.is_multiple_of(scale), "multisample storage dimensions")?;
            if samples == 4 {
                require(u64::from(width) * u64::from(height) <= 16 * 1024 * 1024, "multisample storage budget")?;
                require(!pitch && matches!(format, soju_switch_texture::format::Format::Rgba8Unorm | soju_switch_texture::format::Format::Bgra8Unorm), "multisample color format/layout")?;
            }
            let mut target = Target {
                address: r.address(base),
                width,
                height,
                block_height: (layout >> 4) & 7,
                pitch,
                generation: 0,
                native_generation: 0,
                memory_identity: memory.identity,
                format,
                depth_rotation: 0,
            };
            let key = format!(
                "{}-{:x}-{}-{width}-{height}-{layout}-s{samples}",
                memory.identity,
                target.address,
                format.info().metal
            );
            require(!batch_targets.contains_key(&key), "aliased MRT attachments")?;
            let initial = capture(memory, &mut dependencies, target.address, target.size())?;
            for (other_key, other) in self
                .targets
                .iter()
                .chain(batch_targets.iter())
                .chain(self.depth_targets.iter())
            {
                require(
                    other_key == &key
                        || target.address >= other.address + other.size() as u64
                        || other.address >= target.address + target.size() as u64,
                    "overlapping target views require a batch flush",
                )?;
            }
            let generation = memory.generation(target.address, target.size())?;
            target.generation = generation;
            target.native_generation = self
                .output_generations
                .get(&key)
                .filter(|(memory_generation, _, _)| *memory_generation == generation)
                .map_or(generation, |(_, native, _)| *native);
            let upload_key = (key.clone(), generation);
            let file = if let Some((file, _)) = self.attachment_files.get(&upload_key) {
                file.clone()
            } else if let Some(files) = self.retained_attachments(&key, generation, 1) {
                let file = files[0].clone();
                self.attachment_files
                    .insert(upload_key, (file.clone(), String::new()));
                file
            } else {
                let initial = if pitch {
                    initial.to_vec()
                } else {
                    target.surface().deswizzle(initial).map_err(io)?
                };
                let initial = if format == soju_switch_texture::format::Format::A1Bgr5Unorm {
                    initial
                        .chunks_exact(2)
                        .flat_map(|p| {
                            u16::from_le_bytes([p[0], p[1]])
                                .rotate_left(1)
                                .to_le_bytes()
                        })
                        .collect()
                } else {
                    initial
                };
                let file = self.file(initial);
                self.attachment_files
                    .insert(upload_key, (file.clone(), String::new()));
                file
            };
            let metal = format.info().metal;
            target_commands.push(TargetCommand {
                key: key.clone().into(),
                width: (width / scale) as _,
                height: (height / scale) as _,
                samples: samples as _,
                generation: target.native_generation as _,
                metal: metal as _,
                bytes: bytes_per_pixel as _,
                file: file.into(),
            });
            batch_targets.insert(key, target);
        }
        let target = batch_targets.values().next().unwrap();
        let (width, height) = (target.width, target.height);
        require(
            batch_targets
                .values()
                .all(|t| t.width == width && t.height == height),
            "MRT size mismatch",
        )?;
        let mut pending_depth = None;
        let depth = if r.get(0x1538) != 0 {
            require(
                r.get(0x1538) == 1 && matches!(r.get(0xfe8), 0x0a | 0x19 | 0x13 | 0x14 | 0x16),
                "unsupported depth format",
            )?;
            require(
                r.get(0x1228) == width
                    && r.get(0x122c) == height
                    && matches!(r.get(0x1230), 1 | 0x10001)
                    && r.get(0xfec) & !0x70 == 0
                    && ((r.get(0xfec) >> 4) & 7) <= 5,
                "depth target layout mismatch",
            )?;
            use soju_switch_texture::format::Format;
            let format = match r.get(0xfe8) {
                0x13 => Format::Depth16Unorm,
                0x14 | 0x16 => Format::Depth24UnormStencil8,
                0x19 => Format::Depth32FloatStencil8,
                _ => Format::Depth32Float,
            };
            require(samples == 1 || format != Format::Depth16Unorm, "multisample depth16 transfer unsupported")?;
            let mut t = Target {
                address: r.address(0xfe0),
                width,
                height,
                block_height: (r.get(0xfec) >> 4) & 7,
                pitch: false,
                generation: 0,
                native_generation: 0,
                memory_identity: memory.identity,
                format,
                depth_rotation: if r.get(0xfe8) == 0x16 { 8 } else { 0 },
            };
            t.generation = memory.generation(t.address, t.size())?;
            let key = format!(
                "{}-{:x}-{width}-{height}-{}-{}-s{samples}",
                memory.identity,
                t.address,
                r.get(0xfe8),
                t.block_height
            );
            for (other_key, other) in self
                .targets
                .iter()
                .chain(batch_targets.iter())
                .chain(self.depth_targets.iter())
            {
                require(
                    other_key == &key
                        || t.address >= other.address + other.size() as u64
                        || other.address >= t.address + t.size() as u64,
                    "overlapping attachment views require readback",
                )?;
            }
            t.native_generation = self
                .output_generations
                .get(&key)
                .filter(|(memory_generation, _, _)| *memory_generation == t.generation)
                .map_or(t.generation, |(_, native, _)| *native);
            // Record the dependency even when the packed attachment is already captured.
            let initial_bytes = capture(memory, &mut dependencies, t.address, t.size())?;
            let upload_key = (key.clone(), t.generation);
            let (zfile, sfile) = if let Some(files) = self.attachment_files.get(&upload_key) {
                files.clone()
            } else if let Some(files) = self.retained_attachments(&key, t.generation, 2) {
                let files = (files[0].clone(), files[1].clone());
                self.attachment_files.insert(upload_key, files.clone());
                files
            } else {
                let initial = t.surface().deswizzle(initial_bytes).map_err(io)?;
                let mut z = Vec::new();
                let mut stencil = Vec::new();
                for p in initial.chunks_exact(format.info().bytes) {
                    match format {
                        Format::Depth16Unorm => z.extend(&p[..2]),
                        Format::Depth24UnormStencil8 => {
                            let packed = u32::from_le_bytes([p[0], p[1], p[2], p[3]])
                                .rotate_left(t.depth_rotation);
                            z.extend(((packed >> 8) as f32 / 16777215.).to_le_bytes());
                            stencil.push(packed.to_le_bytes()[0]);
                        }
                        Format::Depth32FloatStencil8 => {
                            z.extend(&p[..4]);
                            stencil.push(p[4]);
                        }
                        _ => z.extend(&p[..4]),
                    }
                }
                let zfile = self.file(z);
                let sfile = self.file(stencil);
                self.attachment_files
                    .insert(upload_key, (zfile.clone(), sfile.clone()));
                (zfile, sfile)
            };
            let command = DepthCommand {
                key: key.clone().into(),
                format: if r.get(0xfe8) == 0x16 {
                    0x14
                } else {
                    r.get(0xfe8) as _
                },
                generation: t.native_generation as _,
                zfile: zfile.into(),
                sfile: sfile.into(),
            };
            pending_depth = Some((key, t));
            vec![command]
        } else {
            require(
                r.get(0x12cc) == 0 && r.get(0x12e8) == 0 && r.get(0x1380) == 0,
                "depth state without zeta",
            )?;
            Vec::new()
        };
        let mut command = Command {
            registers: r
                .0
                .iter()
                .map(|(&address, &value)| Reg {
                    address: address as _,
                    value: value as _,
                })
                .collect::<Vec<_>>()
                .into(),
            targets: target_commands.into(),
            depth: depth.into(),
            repeats: 1,
            ..Command::default()
        };
        self.checkpoint(&mut clock, 0);
        match *action {
            Action::ResetOcclusion | Action::ReportOcclusion { .. } => unreachable!(),
            Action::Clear(mask) => {
                require(
                    mask != 0
                        && mask & !0x3ff == 0
                        && (mask >> 6) < attachment_count
                        && matches!(mask & 60, 0 | 60),
                    "partial-channel/layer clear unsupported",
                )?;
                require(
                    mask & 3 == 0 || !command.depth.is_empty(),
                    "zeta clear without target",
                )?;
                require(
                    mask & 2 == 0 || matches!(r.get(0xfe8), 0x19 | 0x14),
                    "stencil clear needs stencil target",
                )?;
                command.kind = 1;
                command.clear = mask as _;
            }
            Action::Draw {
                topology,
                first,
                count,
                indexed,
                instance,
            } => {
                require(r.get(0xd7c) <= 1, "invalid clip depth convention")?;
                require(count > 0 && count <= 1_000_000, "invalid draw count")?;
                require(
                    r.get(0x2100) & 1 == 0,
                    "geometry stage/OUT emulation is not implemented",
                )?;
                for stage in [0, 2, 3] {
                    require(
                        r.get(0x2000 + stage * 64) & 1 == 0,
                        "unsupported enabled shader stage",
                    )?;
                }
                require(
                    r.get(0x2040) == 0x11 && r.get(0x2140) == 0x51,
                    "Vertex B and fragment stages must be enabled",
                )?;
                let rectangle = r.get(0x113c) == 2;
                require(r.get(0x113c) == 0 || rectangle, "FILL_ALL is not implemented")?;
                if rectangle {
                    require(topology == 4 && !indexed && first.is_multiple_of(3) && count.is_multiple_of(3), "rectangle requires complete nonindexed triangles")?;
                }
                let base = r.address(0x1608);
                let va = base
                    .checked_add(u64::from(r.get(0x2044)))
                    .ok_or_else(|| Error("shader address overflow".into()))?;
                let fa = base
                    .checked_add(u64::from(r.get(0x2144)))
                    .ok_or_else(|| Error("shader address overflow".into()))?;
                for address in [va, fa] {
                    if let Some((size, _)) = self.programs.get(&address) {
                        dependencies.push((address, *size, memory.generation(address, *size)?));
                    }
                }
                let (mut vs, vh, vkey) = self.shader(va, memory, Stage::Vertex)?;
                let (mut fs, fh, fkey) = self.shader(fa, memory, Stage::Fragment)?;
                for shader in [&mut vs, &mut fs] {
                    if !shader.uses_texture_table {
                        continue;
                    }
                    let mut kinds = BTreeSet::new();
                    for binding in &shader.texture_bindings {
                        require(
                            binding.texture_index <= r.get(0x157c)
                                && binding.sampler_index <= r.get(0x1564),
                            "texture pool index out of bounds",
                        )?;
                        let tic_address = r
                            .address(0x1574)
                            .checked_add(u64::from(binding.texture_index) * 32)
                            .ok_or_else(|| Error("TIC address overflow".into()))?;
                        let tic = Tic::parse(capture(memory, &mut dependencies, tic_address, 32)?)?;
                        let integer = tic.format() == soju_switch_texture::format::Format::R32Uint;
                        kinds.insert(integer);
                        if integer {
                            let sampler_address = r
                                .address(0x155c)
                                .checked_add(u64::from(binding.sampler_index) * 32)
                                .ok_or_else(|| Error("TSC address overflow".into()))?;
                            let sampler = Tsc::parse(capture(
                                memory,
                                &mut dependencies,
                                sampler_address,
                                32,
                            )?)?;
                            require(
                                sampler.min_filter == 1 && sampler.mag_filter == 1,
                                "integer sampling requires nearest filtering",
                            )?;
                        }
                    }
                    require(
                        kinds.len() <= 1,
                        "mixed integer/float texture tables unsupported",
                    )?;
                    if kinds.contains(&true) {
                        shader.source = shader
                            .source
                            .replace("texture2d<float>", "texture2d<uint>")
                            .replace("float4 texel =", "uint4 texel =");
                    }
                }
                require(
                    fh.color_masks[..attachment_count as usize]
                        .iter()
                        .any(|x| *x != 0)
                        && fh.color_masks[attachment_count as usize..]
                            .iter()
                            .all(|x| *x == 0),
                    "fragment output target mismatch",
                )?;
                for slot in 0..attachment_count {
                    let physical = (r.get(0x121c) >> (4 + slot * 3)) & 7;
                    if r.get(0x810 + physical * 64) == 0xe4 {
                        require(
                            r.get(0x1360 + physical * 4) == 0,
                            "integer attachment blending unsupported",
                        )?;
                        fs.source = fs.source.replace(
                            &format!("float4 color{slot}"),
                            &format!("uint4 color{slot}"),
                        );
                        for c in 0..4 {
                            fs.source = fs.source.replace(
                                &format!("out.color{slot}[{c}] = as_type<float>"),
                                &format!("out.color{slot}[{c}] = as_type<uint>"),
                            );
                        }
                    }
                }
                // Integer vertex fetch must preserve register bits rather than convert to float.
                for slot in 0..32 {
                    let ty = (r.get(0x1160 + slot * 4) >> 27) & 7;
                    let name = match ty { 3 => "int", 4 => "uint", _ => continue };
                    vs.source = vs.source.replace(
                        &format!("float4 a{slot} [[attribute({slot})]];"),
                        &format!("{name}4 a{slot} [[attribute({slot})]];"),
                    );
                }
                let mut signature = Vec::with_capacity(2048);
                signature.extend(vkey.0);
                signature.extend(fkey.0);
                for value in [
                    topology,
                    r.get(0x15d0),
                    r.get(0x113c),
                    r.get(0x1684),
                    r.get(0x12ec),
                    r.get(0x1310),
                    r.get(0x1314),
                    r.get(0xd7c),
                    r.get(0x1510),
                    r.get(0x1940),
                    r.get(0x1910),
                    r.get(0x1518),
                    u32::from(r.float(0xa00) < 0.),
                    u32::from(r.float(0xa04) < 0.),
                    u32::from(vs.source.contains("texture2d<uint>")),
                    u32::from(fs.source.contains("texture2d<uint>")),
                ] {
                    signature.extend(value.to_le_bytes());
                }
                for index in 0..16 {
                    signature.push(u8::from(r.float(0xa00 + index * 32) < 0.));
                    signature.push(u8::from(r.float(0xa04 + index * 32) < 0.));
                }
                for a in (0..=0x2000).step_by(4) {
                    let descriptor = matches!(a, 0x810 | 0x850 | 0x890 | 0x8d0 | 0x910 | 0x950 | 0x990 | 0x9d0 | 0x121c | 0x1e00..=0x1eff | 0xfe8 | 0x1538 | 0xf54..=0xf5c | 0x1160..=0x11dc | 0x12cc | 0x12e4 | 0x12e8 | 0x130c | 0x133c..=0x139c | 0x1594..=0x15a4 | 0x1880..=0x18fc)
                        || ((0x1c00..=0x1dff).contains(&a) && matches!(a % 16, 0 | 12));
                    if descriptor {
                        signature.extend((a as u32).to_le_bytes());
                        signature.extend(r.get(a as u32).to_le_bytes());
                    }
                }
                let (vs, fs, pipeline) = if let Some(cached) = self.pipeline_states.get(&signature)
                {
                    cached.clone()
                } else {
                    if r.get(0x12ec) != 0 {
                        require(r.get(0x12ec) == 1 && fh.color_masks[0] & 8 != 0 && !fs.source.contains("uint4 color0"), "alpha test requires floating color0 alpha")?;
                        let op = match r.get(0x1314) {
                            1..=8 => r.get(0x1314) - 1,
                            0x200..=0x207 => r.get(0x1314) - 0x200,
                            _ => return Err(Error("alpha comparison function".into())),
                        };
                        let reference = format!("as_type<float>({}u)", r.get(0x1310));
                        let condition = match op {
                            0 => "false".into(), 7 => "true".into(),
                            _ => format!("out.color0.a {} {reference}", ["", "<", "==", "<=", ">", "!=", ">="][op as usize]),
                        };
                        let start = fs.source.find("fragment StageOut soju_fragment").ok_or_else(|| Error("fragment entry missing".into()))?;
                        fs.source = format!("{}{}", &fs.source[..start], fs.source[start..].replace("return out;", &format!("if (!({condition})) discard_fragment(); return out;")));
                    }
                    let mut finish = String::new();
                    let viewport_array = vh.outputs.iter().any(|a| a.address == 0x68);
                    for (axis, reg) in [("x", 0xa00), ("y", 0xa04)] {
                        if viewport_array {
                            for index in 0..16 {
                                if r.float(reg + index * 32) < 0. {
                                    writeln!(finish, "if(out.viewport_index == {index}u) out.position.{axis} = -out.position.{axis};").unwrap();
                                }
                            }
                        } else if r.float(reg) < 0. {
                            writeln!(finish, "out.position.{axis} = -out.position.{axis};")
                                .unwrap();
                        }
                    }
                    if r.get(0xd7c) == 0 {
                        finish.push_str(
                            "out.position.z = (out.position.z + out.position.w) * 0.5f;\n",
                        );
                    }
                    require(
                        r.get(0x1510) <= 255 && r.get(0x1940) == 0,
                        "unsupported clip/cull mask",
                    )?;
                    let has_clip = vh
                        .outputs
                        .iter()
                        .any(|a| (0x2c0..0x2e0).contains(&a.address));
                    for i in 0..8 {
                        if has_clip
                            && (r.get(0x1510) & (1 << i) == 0
                                || !vh.outputs.iter().any(|a| a.address == 0x2c0 + i * 4))
                        {
                            writeln!(finish, "out.clip_distance[{i}]=1.0f;").unwrap();
                        }
                    }
                    require(
                        topology != 0 || r.get(0x1910) == 0,
                        "shader-selected point size slot unsupported",
                    )?;
                    if topology == 0 {
                        let size = r.float(0x1518);
                        require(size.is_finite() && size > 0., "invalid point size")?;
                        if !vh.outputs.iter().any(|a| a.address == 0x6c) {
                            vs.source = vs.source.replace(
                                "float4 position [[position]];",
                                "float4 position [[position]]; float point_size [[point_size]];",
                            );
                        }
                        writeln!(finish, "out.point_size={size:?}f;").unwrap();
                    }
                    finish.push_str("return out;");
                    let entry = vs
                        .source
                        .find("vertex StageOut soju_vertex")
                        .ok_or_else(|| Error("vertex entry missing".into()))?;
                    vs.source = format!(
                        "{}{}",
                        &vs.source[..entry],
                        vs.source[entry..].replace("return out;", &finish)
                    );
                    if rectangle {
                        require(self.programs[&va].1.global_memory.is_none(), "rectangle vertex global-memory side effects unsupported")?;
                        fill_rectangle::wrap(&mut vs, &vh, &fh, r.get(0x1684) == 1)?;
                    }
                    let mut cache_state = signature.clone();
                    cache_state.extend(vs.source.as_bytes());
                    cache_state.extend(fs.source.as_bytes());
                    // Canonical descriptor state: absent registers and explicit zero agree.
                    // Addresses, draw arguments, viewports and clears are dynamic state.
                    let pipeline = Key::new(Inputs {
                        program: &vkey.0,
                        header: &fkey.0,
                        render_state: &cache_state,
                    });
                    let pipeline = pipeline
                        .0
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>();
                    let weight = signature.len() + vs.source.len() + fs.source.len() + 1024;
                    self.pipeline_states.insert(
                        signature,
                        (vs.clone(), fs.clone(), pipeline.clone()),
                        weight,
                    );
                    (vs, fs, pipeline)
                };
                let vfile = self.file(vs.source.as_bytes().to_vec());
                let ffile = self.file(fs.source.as_bytes().to_vec());
                self.checkpoint(&mut clock, 1);
                let mut constants = Vec::new();
                for (stage, shader, group_reg) in [(0, &vs, 0x2050), (1, &fs, 0x2150)] {
                    let group = r.get(group_reg) as usize;
                    require(group < 5, "constant binding group out of range")?;
                    for slot in &shader.constant_buffers {
                        let cb = bindings[group][*slot as usize]
                            .ok_or_else(|| Error(format!("unbound cbuf {stage}:{slot}")))?;
                        require(cb.size > 0, "empty constant buffer")?;
                        let address = if stage == 0 { va } else { fa };
                        let declared =
                            self.programs[&address].1.constant_buffer_bytes[*slot as usize];
                        require(
                            declared == 0 || declared <= cb.size,
                            "constant buffer shrank below compiled bounds",
                        )?;
                        let offset = uniform_offset(
                            &mut self.constant_upload,
                            &mut self.uniform_offsets,
                            cb,
                            memory,
                        )?;
                        captured_constants.push(cb);
                        constants.push(ConstantCommand {
                            stage,
                            slot: *slot as _,
                            offset: offset as _,
                        });
                    }
                }
                let base_vertex = r.get(0x1434) as i32;
                let base_instance = r
                    .get(0x1438)
                    .checked_add(instance)
                    .ok_or_else(|| Error("base instance overflow".into()))?;
                let mut segments = Vec::new();
                let (index_file, min_vertex, max_vertex, index_size) = if topology == 7 {
                    require(
                        !indexed && count.is_multiple_of(4) && base_vertex == 0,
                        "only complete nonindexed quads supported",
                    )?;
                    require(
                        !fh.inputs.iter().any(|a| {
                            a.interpolation == Some(soju_switch_shader::header::Interpolation::Flat)
                        }),
                        "flat-shaded quad provoking vertex unsupported",
                    )?;
                    let max = first
                        .checked_add(count - 1)
                        .ok_or_else(|| Error("quad vertex overflow".into()))?;
                    let mut data = Vec::with_capacity(count as usize * 6);
                    for start in (first..=max).step_by(4) {
                        for corner in [0, 1, 2, 0, 2, 3] {
                            data.extend((start + corner).to_le_bytes());
                        }
                    }
                    segments.push((0, (count / 4 * 6) as usize));
                    (self.file(data), first, max, 4)
                } else if indexed {
                    let size = match r.get(0x17d8) {
                        0 => 1,
                        1 => 2,
                        2 => 4,
                        _ => return Err(Error("index format".into())),
                    };
                    let address = r
                        .address(0x17c8)
                        .checked_add(u64::from(first) * size)
                        .ok_or_else(|| Error("index address overflow".into()))?;
                    let end = address
                        .checked_add(u64::from(count) * size)
                        .ok_or_else(|| Error("index range overflow".into()))?;
                    require(
                        end > address && end - 1 <= r.address(0x17d0),
                        "index buffer limit exceeded",
                    )?;
                    let bytes = capture(
                        memory,
                        &mut dependencies,
                        address,
                        (u64::from(count) * size) as usize,
                    )?;
                    let indices = bytes
                        .chunks(size as usize)
                        .map(|b| {
                            let mut word = [0; 4];
                            word[..b.len()].copy_from_slice(b);
                            u32::from_le_bytes(word)
                        })
                        .collect::<Vec<_>>();
                    require(r.get(0x1644) <= 1, "invalid primitive restart enable")?;
                    let restart = r.get(0x1644) == 1;
                    let marker = r.get(0x1648);
                    let mut valid = Vec::new();
                    let mut start = 0;
                    for (i, &v) in indices.iter().enumerate() {
                        if restart && v == marker {
                            if i > start {
                                segments.push((start, i - start));
                            }
                            start = i + 1;
                        } else {
                            valid.push(v);
                        }
                    }
                    if start < indices.len() {
                        segments.push((start, indices.len() - start));
                    }
                    let min = i64::from(*valid.iter().min().unwrap_or(&0)) + i64::from(base_vertex);
                    let max = i64::from(*valid.iter().max().unwrap_or(&0)) + i64::from(base_vertex);
                    require(
                        min >= 0 && max <= u32::MAX as i64,
                        "base vertex outside range",
                    )?;
                    let (data, metal_size) = if size == 1 {
                        (
                            indices
                                .iter()
                                .flat_map(|v| (*v as u16).to_le_bytes())
                                .collect(),
                            2,
                        )
                    } else {
                        (bytes.to_vec(), size)
                    };
                    (self.file(data), min as u32, max as u32, metal_size)
                } else {
                    require(
                        base_vertex == 0,
                        "base vertex on nonindexed draw unsupported",
                    )?;
                    (
                        String::new(),
                        first,
                        first
                            .checked_add(count - 1)
                            .ok_or_else(|| Error("vertex count overflow".into()))?,
                        0,
                    )
                };
                command.segments = segments
                    .iter()
                    .map(|&(start, count)| SegmentCommand {
                        start: start as _,
                        count: count as _,
                    })
                    .collect::<Vec<_>>()
                    .into();
                let _ = min_vertex;
                let slots = vh
                    .inputs
                    .iter()
                    .filter(|a| !matches!(a.address, 0x2f8 | 0x2fc))
                    .map(|a| (a.address / 16 - 8) as u32)
                    .collect::<BTreeSet<_>>();
                let mut attrs = Vec::new();
                let mut streams = BTreeMap::<u32, u32>::new();
                let mut stream_slots = BTreeMap::<u32, u32>::new();
                let mut occupied: BTreeSet<u32> =
                    vs.constant_buffers.iter().copied().map(u32::from).collect();
                let options = &self.programs[&va].1;
                if vs.uses_texture_table {
                    occupied.insert(u32::from(options.texture_buffer));
                }
                if let Some(global) = options.global_memory {
                    occupied.insert(u32::from(global.buffer));
                }
                for slot in slots {
                    if rectangle {
                        let stream = r.get(0x1160 + slot * 4) & 31;
                        require(r.get(0x1880 + stream * 4) == 1, "rectangle per-vertex attributes require vertex pulling")?;
                    }
                    require(slot < 32, "vertex attribute slot")?;
                    let a = r.get(0x1160 + slot * 4);
                    let stream = a & 31;
                    let offset = (a >> 7) & 0x3fff;
                    let bits = (a >> 21) & 63;
                    let ty = (a >> 27) & 7;
                    require(
                        a & 0x80000040 == 0,
                        "inactive/swapped vertex attribute unsupported",
                    )?;
                    let (format, bytes) = match (bits, ty) {
                        (1, 7) => (31, 16),
                        (2, 7) => (30, 12),
                        (4, 7) => (29, 8),
                        (0x12, 7) => (28, 4),
                        (0xa | 0x2f, 2) => (9, 4),
                        (0x1b, 4 | 5) => (49, 2),
                        (0x18, 4) => (1, 2),
                        (0x1b, 3) => (50, 2),
                        _ => return Err(Error(format!("vertex format {bits}/{ty} unsupported"))),
                    };
                    let stream_format = r.get(0x1c00 + stream * 16);
                    let stride = stream_format & 4095;
                    require(
                        stream_format & !0x1fff == 0
                            && stream_format & 4096 != 0
                            && stride > 0
                            && offset + bytes <= stride,
                        "invalid vertex stream stride/enable",
                    )?;
                    let metal_slot = if let Some(&slot) = stream_slots.get(&stream) {
                        slot
                    } else {
                        let slot =
                            (0..31)
                                .find(|slot| !occupied.contains(slot))
                                .ok_or_else(|| {
                                    Error("vertex buffers exceed Metal binding capacity".into())
                                })?;
                        occupied.insert(slot);
                        stream_slots.insert(stream, slot);
                        slot
                    };
                    let instanced = r.get(0x1880 + stream * 4);
                    require(instanced <= 1, "invalid instancing mode")?;
                    let rate = if instanced == 1 {
                        r.get(0x1c0c + stream * 16)
                    } else {
                        1
                    };
                    require(rate > 0, "zero instance divisor")?;
                    let last = if instanced == 1 {
                        base_instance / rate
                    } else {
                        max_vertex
                    };
                    let length = u64::from(last) * u64::from(stride) + u64::from(offset + bytes);
                    require(length <= 64 * 1024 * 1024, "vertex upload exceeds 64 MiB")?;
                    streams
                        .entry(stream)
                        .and_modify(|n| *n = (*n).max(length as u32))
                        .or_insert(length as u32);
                    attrs.push(AttributeCommand {
                        slot: slot as _,
                        stream: i64::from(metal_slot),
                        offset: offset as _,
                        format: format as _,
                        stride: stride as _,
                        instanced: instanced as _,
                        rate: rate as _,
                    });
                }
                let mut buffers = Vec::new();
                for (stream, length) in streams {
                    let address = r.address(0x1c04 + stream * 16);
                    let end = address
                        .checked_add(u64::from(length))
                        .ok_or_else(|| Error("vertex address overflow".into()))?;
                    require(
                        end > address && end - 1 <= r.address(0x1f00 + stream * 8),
                        "vertex stream limit exceeded",
                    )?;
                    // Geometric capture sizes share a buffer across adjacent draws without
                    // reading beyond either the guest-declared limit or the mapped region.
                    let available = memory.available(address)?;
                    let limit = r
                        .address(0x1f00 + stream * 8)
                        .saturating_sub(address)
                        .saturating_add(1);
                    let captured = (length as usize)
                        .next_power_of_two()
                        .min(available)
                        .min(limit.min(usize::MAX as u64) as usize);
                    require(captured >= length as usize, "unmapped vertex stream range")?;
                    let file = self.resource_file(memory, &mut dependencies, address, captured)?;
                    buffers.push(BufferCommand {
                        slot: i64::from(stream_slots[&stream]),
                        file: file.into(),
                    });
                }
                let mut textures = Vec::new();
                for (stage, shader, address) in [(0, &vs, va), (1, &fs, fa)] {
                    if !shader.uses_texture_table {
                        continue;
                    }
                    require(
                        r.get(0xf10) == 1,
                        "Maxwell texture headers must be selected",
                    )?;
                    let options = &self.programs[&address].1;
                    let count = options.texture_count;
                    require(
                        shader.texture_bindings.iter().all(|b| {
                            matches!(b.texture_type, soju_switch_shader::texture::TextureType::D2 | soju_switch_shader::texture::TextureType::D2Array | soju_switch_shader::texture::TextureType::Shadow2D | soju_switch_shader::texture::TextureType::Shadow2DArray)
                        }),
                        "only 2D/array texture argument tables supported",
                    )?;
                    require(
                        !shader.texture_bindings.is_empty(),
                        "bindless resources require host-normalized bindings",
                    )?;
                    let mut images = BTreeSet::new();
                    let mut samplers = BTreeSet::new();
                    for b in &shader.texture_bindings {
                        images.insert((b.texture_index, b.texture_type));
                        samplers.insert(b.sampler_index);
                    }
                    for (index, texture_type) in images {
                        require(index <= r.get(0x157c), "TIC pool index out of bounds")?;
                        let addr = r
                            .address(0x1574)
                            .checked_add(u64::from(index) * 32)
                            .ok_or_else(|| Error("TIC address overflow".into()))?;
                        let tic = Tic::parse(capture(memory, &mut dependencies, addr, 32)?)?;
                        use soju_switch_shader::texture::TextureType;
                        let array = matches!(texture_type, TextureType::D2Array | TextureType::Shadow2DArray);
                        let shadow = matches!(texture_type, TextureType::Shadow2D | TextureType::Shadow2DArray);
                        require(tic.array == array, "shader/TIC array type mismatch")?;
                        let texture_end = tic
                            .address
                            .checked_add(tic.surface().size() as u64)
                            .ok_or_else(|| Error("texture range overflow".into()))?;
                        for rendered in self
                            .targets
                            .values()
                            .chain(batch_targets.values())
                            .chain(self.depth_targets.values())
                            .chain(pending_depth.iter().map(|(_, t)| t))
                        {
                            require(
                                tic.address >= rendered.address + rendered.size() as u64
                                    || rendered.address >= texture_end,
                                "render-to-texture feedback requires execute before prepare",
                            )?;
                        }
                        dependencies.push((
                            tic.address,
                            tic.surface().size(),
                            memory.generation(tic.address, tic.surface().size())?,
                        ));
                        let mut data = self.textures.upload(&tic, memory)?;
                        let info = tic.format().info();
                        let mut metal = info.metal;
                        let mut row = tic.surface().width * info.bytes;
                        use soju_switch_texture::format::Format;
                        if matches!(
                            tic.format(),
                            Format::Depth16Unorm
                                | Format::Depth32Float
                                | Format::Depth24UnormStencil8
                                | Format::Depth32FloatStencil8
                        ) {
                            data = data
                                .chunks_exact(info.bytes)
                                .flat_map(|p| {
                                    let z = match tic.format() {
                                        Format::Depth16Unorm => {
                                            u16::from_le_bytes([p[0], p[1]]) as f32 / 65535.
                                        }
                                        Format::Depth24UnormStencil8 => {
                                            (u32::from_le_bytes(p[..4].try_into().unwrap()) >> 8)
                                                as f32
                                                / 16777215.
                                        }
                                        _ => f32::from_le_bytes(p[..4].try_into().unwrap()),
                                    };
                                    z.to_le_bytes()
                                })
                                .collect();
                            metal = if shadow { 252 } else { 55 };
                            row = tic.width as usize * 4;
                        }
                        require(!shadow || metal == 252, "comparison texture must contain depth")?;
                        let mut swizzle = tic.swizzle();
                        if matches!(tic.format(), Format::B5G6R5Unorm | Format::A1Bgr5Unorm) {
                            for c in &mut swizzle {
                                *c = match *c {
                                    2 => 4,
                                    4 => 2,
                                    v => v,
                                };
                            }
                            if tic.format() == Format::A1Bgr5Unorm {
                                data = data
                                    .chunks_exact(2)
                                    .flat_map(|p| {
                                        u16::from_le_bytes([p[0], p[1]])
                                            .rotate_left(1)
                                            .to_le_bytes()
                                    })
                                    .collect();
                            }
                        }
                        let key = Key::new(Inputs {
                            program: &data,
                            header: &tic
                                .raw
                                .into_iter()
                                .flat_map(u32::to_le_bytes)
                                .collect::<Vec<_>>(),
                            render_state: &[u8::from(shadow)],
                        });
                        let key = key.0.iter().map(|v| format!("{v:02x}")).collect::<String>();
                        let file = self.file(data);
                        textures.push(TextureCommand {
                            stage,
                            count: count as _,
                            image: (index + if shadow { if array { 8 * count } else { 7 * count } } else if array { 5 * count } else { 0 }) as _,
                            layers: tic.layers as _,
                            array: i64::from(array),
                            sampler: -1,
                            key: key.into(),
                            width: tic.width as _,
                            height: tic.height as _,
                            metal: metal as _,
                            row: row as _,
                            rows: tic.surface().height as _,
                            swizzle0: swizzle[0] as _,
                            swizzle1: swizzle[1] as _,
                            swizzle2: swizzle[2] as _,
                            swizzle3: swizzle[3] as _,
                            file: file.into(),
                            ..TextureCommand::default()
                        });
                    }
                    for index in samplers {
                        require(index <= r.get(0x1564), "TSC pool index out of bounds")?;
                        let addr = r
                            .address(0x155c)
                            .checked_add(u64::from(index) * 32)
                            .ok_or_else(|| Error("TSC address overflow".into()))?;
                        let t = Tsc::parse(capture(memory, &mut dependencies, addr, 32)?)?;
                        textures.push(TextureCommand {
                            stage,
                            count: count as _,
                            image: -1,
                            sampler: index as _,
                            u: t.address_u as _,
                            v: t.address_v as _,
                            min: t.min_filter as _,
                            mag: t.mag_filter as _,
                            anisotropy: t.max_anisotropy as _,
                            compare: t.compare.map_or(-1, i64::from),
                            ..TextureCommand::default()
                        });
                    }
                }
                command.pipeline = pipeline.into();
                command.vertex = vfile.into();
                command.fragment = ffile.into();
                command.attributes = attrs.into();
                command.buffers = buffers.into();
                command.constants = constants.into();
                command.textures = textures.into();
                command.topology = if topology == 7 { 4 } else { topology as _ };
                command.first = i64::from(first) * if rectangle { 2 } else { 1 };
                command.count = i64::from(count) * if rectangle { 2 } else { 1 };
                command.indices = index_file.into();
                command.index_size = index_size as _;
                command.base_vertex = base_vertex as _;
                command.instance = base_instance as _;
            }
        }
        self.checkpoint(&mut clock, 2);
        if matches!(action, Action::Draw { .. }) && r.get(0x1514) == 0 {
            self.prepared = Some((
                Snapshot {
                    registers: r.clone(),
                    constants: *bindings,
                    action: action.clone(),
                },
                memory.epoch(),
                self.commands.len(),
            ));
        }
        self.commands.push(command);
        self.targets.extend(batch_targets);
        if let Some((key, t)) = pending_depth {
            self.depth_targets.insert(key, t);
        }
        if let Some((state, epoch, index)) = &self.prepared {
            dependencies.sort_unstable();
            dependencies.dedup();
            if self.variants.len() == 64 {
                self.variants.remove(0);
            }
            self.variants.push(PreparedVariant {
                state: state.clone(),
                epoch: *epoch,
                index: *index,
                dependencies,
                constants: captured_constants,
                views: (self.targets.len(), self.depth_targets.len()),
                vertex_capacity: vertex_capacity(&self.commands[*index], &self.files),
            });
        }
        Ok(())
    }
    /// Queue a captured batch on the persistent GPU thread. No image readback occurs.
    /// The caller must synchronize before CPU/transfer access to pending targets.
    pub fn submit(&mut self, directory: &Path) -> Result<crate::worker::Fence> {
        self.serial += 1;
        let directory = directory.join(format!("batch-{}", self.serial));
        std::fs::create_dir_all(&directory).map_err(io)?;
        let directory = std::fs::canonicalize(directory).map_err(io)?;
        require(
            !directory.to_string_lossy().contains(['\n', '\t']),
            "invalid worker directory",
        )?;
        let commands = std::mem::take(&mut self.commands);
        let files = std::mem::take(&mut self.files);
        let constants = std::mem::take(&mut self.constant_upload);
        let fence = self
            .worker
            .get_or_insert_with(crate::worker::Worker::new)
            .submit(directory, commands, files, constants)?;
        self.prepared = None;
        self.commands.clear();
        self.variants.clear();
        self.files.clear();
        self.file_cache.clear();
        self.resource_files.clear();
        self.attachment_files.clear();
        self.constant_upload.clear();
        self.uniform_offsets.clear();
        Ok(fence)
    }
    /// Explicit CPU visibility boundary. Waits for queued work and writes pending
    /// color/depth targets and query results to guest memory. Fence waits alone never copy image bytes.
    pub fn readback(&mut self, directory: &Path, memory: &mut Memory) -> Result<Report> {
        self.readback_inner(directory, memory, true)
    }
    fn readback_inner(
        &mut self,
        directory: &Path,
        memory: &mut Memory,
        capture_images: bool,
    ) -> Result<Report> {
        let _span = crate::profile::span(crate::profile::Stage::Readback);
        require(
            self.commands.is_empty(),
            "submit prepared commands before readback",
        )?;
        for t in self.compute_targets.values() {
            require(
                memory.identity == t.identity
                    && memory.generation(t.address, t.size)? == t.generation,
                "CPU write overlaps pending compute; synchronize before writing",
            )?;
        }
        for t in self.targets.values().chain(self.depth_targets.values()) {
            require(
                memory.identity == t.memory_identity
                    && memory.generation(t.address, t.size())? == t.generation,
                "CPU write overlaps pending target; synchronize before writing",
            )?;
        }
        for &(address, generation, identity) in self.queries.values() {
            require(
                memory.identity == identity && memory.generation(address, 4)? == generation,
                "CPU write overlaps pending query",
            )?;
        }
        std::fs::create_dir_all(directory).map_err(io)?;
        let directory = std::fs::canonicalize(directory).map_err(io)?;
        require(
            !directory.to_string_lossy().contains(['\n', '\t']),
            "invalid worker directory",
        )?;
        let reply = self
            .worker
            .as_mut()
            .ok_or_else(|| Error("no submitted work".into()))?
            .request("readback", directory.clone())?
            .readback()?;
        let diagnostics = reply.diagnostics;
        let mut readbacks = reply.readbacks;
        let mut take = |name: String| {
            readbacks
                .remove(&name)
                .ok_or_else(|| Error(format!("missing native readback {name}")))
        };
        let mut images = Vec::new();
        let mut writes = Vec::new();
        let retain = self.queries.is_empty() && self.compute_targets.is_empty();
        let mut retained = std::collections::HashMap::new();
        let mut retained_bytes = 0usize;
        let mut retain_files = |key: &String, files: &[&[u8]]| {
            let size = files
                .iter()
                .map(|f| f.len().max(1).next_multiple_of(16384))
                .sum::<usize>();
            if retain && retained_bytes + size <= 64 * 1024 * 1024 {
                retained_bytes += size;
                retained.insert(
                    key.clone(),
                    files
                        .iter()
                        .map(|f| crate::upload::Upload::copy(f))
                        .collect::<Vec<_>>(),
                );
            }
        };
        for (key, t) in &self.targets {
            let bytes = take(format!("{key}.rgba"))?;
            retain_files(key, &[&bytes]);
            let bytes = if t.format == soju_switch_texture::format::Format::A1Bgr5Unorm {
                bytes
                    .chunks_exact(2)
                    .flat_map(|p| {
                        u16::from_le_bytes([p[0], p[1]])
                            .rotate_right(1)
                            .to_le_bytes()
                    })
                    .collect::<Vec<_>>()
            } else {
                bytes
            };
            require(
                bytes.len() == t.width as usize * t.height as usize * t.format.info().bytes,
                "invalid readback length",
            )?;
            let data = t.writeback(&bytes, memory)?;
            memory.read(t.address, data.len())?;
            writes.push((t.address, data));
            if capture_images {
                images.push(Readback {
                    address: t.address,
                    width: t.width,
                    height: t.height,
                    rgba: crate::format::rgba(t.format, &bytes),
                });
            }
        }
        for (key, t) in &self.depth_targets {
            use soju_switch_texture::format::Format;
            let z = take(format!("{key}.depth"))?;
            let stencil = take(format!("{key}.stencil"))?;
            retain_files(key, &[&z, &stencil]);
            let pixels = t.width as usize * t.height as usize;
            let depth_bytes = if t.format == Format::Depth16Unorm {
                2
            } else {
                4
            };
            require(
                z.len() == pixels * depth_bytes && (stencil.is_empty() || stencil.len() == pixels),
                "invalid depth readback",
            )?;
            let raw = match t.format {
                Format::Depth24UnormStencil8 => {
                    require(stencil.len() == pixels, "missing stencil bytes")?;
                    let mut raw = vec![0; pixels * 4];
                    for ((p, value), stencil) in
                        raw.chunks_exact_mut(4).zip(z.chunks_exact(4)).zip(&stencil)
                    {
                        let f = f32::from_le_bytes(value.try_into().unwrap());
                        let v = ((f.clamp(0., 1.) * 16777215.).round() as u32) << 8
                            | u32::from(*stencil);
                        p.copy_from_slice(&v.rotate_right(t.depth_rotation).to_le_bytes());
                    }
                    raw
                }
                Format::Depth32FloatStencil8 => {
                    require(stencil.len() == pixels, "missing stencil bytes")?;
                    // Preserve the three guest padding bytes in every packed texel.
                    let mut raw = t
                        .surface()
                        .deswizzle(memory.read(t.address, t.size())?)
                        .map_err(io)?;
                    for ((p, value), stencil) in
                        raw.chunks_exact_mut(8).zip(z.chunks_exact(4)).zip(&stencil)
                    {
                        p[..4].copy_from_slice(value);
                        p[4] = *stencil;
                    }
                    raw
                }
                _ => z,
            };
            writes.push((t.address, t.writeback(&raw, memory)?));
        }
        for (key, &(address, _, _)) in &self.queries {
            let data = take(format!("{key}.query"))?;
            require(data.len() == 4, "invalid query result")?;
            writes.push((address, data));
        }
        for (key, t) in &self.compute_targets {
            let bytes = take(format!("{key}.buffer"))?;
            require(bytes.len() == t.size, "invalid compute readback size")?;
            writes.push((t.address, bytes));
        }
        for (address, data) in writes {
            memory.write(address, &data)?;
        }
        if retain {
            if self.output_generations.len() > 1024 {
                self.output_generations.clear();
            }
            let previous_bytes: usize = self
                .output_generations
                .values()
                .flat_map(|(_, _, files)| files)
                .map(|f| f.len().max(1).next_multiple_of(16384))
                .sum();
            if previous_bytes + retained_bytes > 64 * 1024 * 1024 {
                for (_, _, files) in self.output_generations.values_mut() {
                    files.clear();
                }
            }
            for (key, t) in self.targets.iter().chain(self.depth_targets.iter()) {
                self.output_generations.insert(
                    key.clone(),
                    (
                        memory.generation(t.address, t.size())?,
                        t.native_generation,
                        retained.remove(key).unwrap_or_default(),
                    ),
                );
            }
        } else {
            // Query/compute outputs may alias a color or depth target's backing.
            self.output_generations.clear();
        }
        self.prepared = None;
        self.commands.clear();
        self.variants.clear();
        self.files.clear();
        self.file_cache.clear();
        self.resource_files.clear();
        self.attachment_files.clear();
        self.constant_upload.clear();
        self.uniform_offsets.clear();
        self.compute_targets.clear();
        self.targets.clear();
        self.depth_targets.clear();
        self.queries.clear();
        Ok(Report {
            images,
            diagnostics,
        })
    }
    /// Publish guest storage without also constructing discarded display-format images.
    pub fn execute_memory(&mut self, directory: &Path, memory: &mut Memory) -> Result<()> {
        let span = crate::profile::span(crate::profile::Stage::Submit);
        self.submit(directory)?.wait()?;
        drop(span);
        self.readback_inner(directory, memory, false)?;
        Ok(())
    }
    /// Compatibility wrapper: submit, wait, then make color targets CPU-visible.
    pub fn execute(&mut self, directory: &Path, memory: &mut Memory) -> Result<Report> {
        let span = crate::profile::span(crate::profile::Stage::Submit);
        let diagnostics = self.submit(directory)?.wait()?;
        drop(span);
        let mut report = self.readback(directory, memory)?;
        report.diagnostics = diagnostics;
        Ok(report)
    }
}
