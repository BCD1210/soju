//! Resource resolution and a dependency-free protocol to the native Metal worker.
use crate::{
    engine::{Action, Snapshot},
    memory::Memory,
    require,
    texture::{TextureCache, Tic, Tsc},
    Error, Result,
};
use soju_switch_shader::{
    cache::{Inputs, Key},
    header::{Header, Stage},
    msl::{MetalShader, Options},
};
use soju_switch_texture::Surface;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
    path::Path,
};

#[derive(Default)]
pub struct Backend {
    programs: BTreeMap<u64, (usize, Options)>,
    pub textures: TextureCache,
    commands: Vec<String>,
    files: Vec<Vec<u8>>,
    targets: BTreeMap<String, Target>,
    worker: Option<crate::worker::Worker>,
    serial: u64,
}
#[derive(Clone)]
struct Target {
    address: u64,
    width: u32,
    height: u32,
    block_height: u32,
    pitch: bool,
    generation: u64,
    memory_identity: u64,
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
fn quote(s: &str) -> String {
    format!(
        "\"{}\"",
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    )
}
impl Target {
    fn surface(&self) -> Surface {
        Surface {
            width: self.width as usize,
            height: self.height as usize,
            depth: 1,
            bytes_per_block: 4,
            block_height_log2: self.block_height,
            block_depth_log2: 0,
        }
    }
    fn size(&self) -> usize {
        if self.pitch {
            self.width as usize * self.height as usize * 4
        } else {
            self.surface().size()
        }
    }
}
impl Backend {
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
        self.programs.insert(address, (size, options));
        Ok(())
    }
    fn file(&mut self, bytes: Vec<u8>) -> String {
        let name = format!("resource-{}.bin", self.files.len());
        self.files.push(bytes);
        quote(&name)
    }
    fn shader(
        &self,
        address: u64,
        memory: &Memory,
        stage: Stage,
    ) -> Result<(MetalShader, Header, Key)> {
        let (size, options) = self
            .programs
            .get(&address)
            .ok_or_else(|| Error(format!("shader extent not registered: {address:#x}")))?;
        let bytes = memory.read(address, *size)?;
        let header = Header::parse(bytes).map_err(io)?;
        require(header.stage == stage, "pipeline stage/SPH mismatch")?;
        let shader = soju_switch_shader::translate(bytes, options).map_err(io)?;
        // MSL includes normalized resource ABI and all compiler-facing translation inputs.
        let key = Key::new(Inputs {
            program: bytes,
            header: &[],
            render_state: shader.source.as_bytes(),
        });
        Ok((shader, header, key))
    }
    /// Resolve immediately at each action, before later CPU/transfer writes.
    pub fn prepare(&mut self, s: &Snapshot, memory: &Memory) -> Result<()> {
        let r = &s.registers;
        for &offset in r.0.keys() {
            let supported = matches!(offset,0x800..=0x9e0|0xa00..=0xa14|0xd74..=0xd90|0xda0|0xdac|0xdb0|0xe00..=0xe08|0xf10|0xf54..=0xf5c|0xfe0..=0xff0|0x1160..=0x11dc|0x121c|0x1228..=0x1230|0x12cc|0x12e4|0x12e8|0x130c|0x131c..=0x1328|0x133c..=0x139c|0x1434|0x1438|0x1538|0x155c..=0x1564|0x1574..=0x157c|0x1594..=0x15a4|0x1608|0x160c|0x1614|0x1618|0x17c8..=0x17e0|0x1880..=0x18fc|0x1918..=0x1920|0x19d0|0x1c00..=0x1eff|0x1f00..=0x1fff|0x2000..=0x2150|0x2380..=0x23cc|0x2410|0x2430|0x2450|0x2470|0x2490);
            require(
                supported,
                &format!("Metal state method {offset:#x} is not implemented"),
            )?;
        }
        let attachment_count = r.get(0x121c) & 15;
        require(
            (1..=8).contains(&attachment_count) && r.get(0x121c) >> 28 == 0,
            "invalid CT_SELECT count",
        )?;
        let mut selected = BTreeSet::new();
        let mut batch_targets = BTreeMap::<String, Target>::new();
        let mut target_jsons = Vec::new();
        for slot in 0..attachment_count {
            let index = (r.get(0x121c) >> (4 + slot * 3)) & 7;
            require(selected.insert(index), "duplicate CT_SELECT target")?;
            let base = 0x800 + index * 64;
            require(
                r.get(base + 0x10) == 0xd5,
                "render target requires A8B8G8R8 UNORM",
            )?;
            require(
                r.get(base + 0x18) == 1 && r.get(base + 0x20) == 0 && r.get(base + 0x1c) == 0,
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
                    r.get(base + 0x08).is_multiple_of(4),
                    "RGBA target pitch alignment",
                )?;
                r.get(base + 0x08) / 4
            } else {
                r.get(base + 0x08)
            };
            let height = r.get(base + 0x0c);
            require(
                width > 0 && height > 0 && width <= 16384 && height <= 16384,
                "invalid render target size",
            )?;
            let mut target = Target {
                address: r.address(base),
                width,
                height,
                block_height: (layout >> 4) & 7,
                pitch,
                generation: 0,
                memory_identity: memory.identity,
            };
            let key = format!(
                "{}-{:x}-d5-{width}-{height}-{layout}",
                memory.identity, target.address
            );
            let initial = memory.read(target.address, target.size())?;
            for (other_key, other) in self.targets.iter().chain(batch_targets.iter()) {
                require(
                    other_key == &key
                        || target.address >= other.address + other.size() as u64
                        || other.address >= target.address + target.size() as u64,
                    "overlapping target views require a batch flush",
                )?;
            }
            let initial = if pitch {
                initial.to_vec()
            } else {
                target.surface().deswizzle(initial).map_err(io)?
            };
            let file = self.file(initial);
            let generation = memory.generation(target.address, target.size())?;
            target.generation = generation;
            let target_json=format!("{{\"key\":{},\"width\":{width},\"height\":{height},\"generation\":{generation},\"file\":{file}}}",quote(&key));
            target_jsons.push(target_json);
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
        let depth = if r.get(0x1538) != 0 {
            require(
                r.get(0x1538) == 1 && matches!(r.get(0xfe8), 0x0a | 0x19),
                "unsupported depth format",
            )?;
            require(
                r.get(0x1228) == width
                    && r.get(0x122c) == height
                    && matches!(r.get(0x1230), 1 | 0x10001)
                    && r.get(0xfec) == 0
                    && r.get(0xff0) == 0,
                "depth target layout mismatch",
            )?;
            format!(
                "{{\"key\":{},\"format\":{},\"generation\":{}}}",
                quote(&format!(
                    "{}-{:x}-{width}-{height}-{}",
                    memory.identity,
                    r.address(0xfe0),
                    r.get(0xfe8)
                )),
                r.get(0xfe8),
                memory.generation(
                    r.address(0xfe0),
                    width as usize * height as usize * if r.get(0xfe8) == 0x19 { 8 } else { 4 }
                )?
            )
        } else {
            require(
                r.get(0x12cc) == 0 && r.get(0x12e8) == 0 && r.get(0x1380) == 0,
                "depth state without zeta",
            )?;
            "null".into()
        };
        let registers =
            r.0.iter()
                .map(|(a, b)| format!("\"{a}\":{b}"))
                .collect::<Vec<_>>()
                .join(",");
        let mut command = format!(
            "{{\"registers\":{{{registers}}},\"targets\":[{}],\"depth\":{depth}",
            target_jsons.join(",")
        );
        match s.action {
            Action::Clear(mask) => {
                require(
                    mask != 0
                        && mask & !0x3ff == 0
                        && (mask >> 6) < attachment_count
                        && matches!(mask & 60, 0 | 60),
                    "partial-channel/layer clear unsupported",
                )?;
                require(
                    mask & 3 == 0 || depth != "null",
                    "zeta clear without target",
                )?;
                require(
                    mask & 2 == 0 || r.get(0xfe8) == 0x19,
                    "stencil clear needs stencil target",
                )?;
                write!(command, ",\"clear\":{mask}").unwrap();
            }
            Action::Draw {
                topology,
                first,
                count,
                indexed,
                instance,
            } => {
                require(
                    r.get(0xd7c) == 1,
                    "only zero-to-positive-W clip depth is implemented",
                )?;
                require(count > 0 && count <= 1_000_000, "invalid draw count")?;
                for stage in [0, 2, 3, 4] {
                    require(
                        r.get(0x2000 + stage * 64) & 1 == 0,
                        "unsupported enabled shader stage",
                    )?;
                }
                require(
                    r.get(0x2040) == 0x11 && r.get(0x2140) == 0x51,
                    "Vertex B and fragment stages must be enabled",
                )?;
                let base = r.address(0x1608);
                let va = base
                    .checked_add(u64::from(r.get(0x2044)))
                    .ok_or_else(|| Error("shader address overflow".into()))?;
                let fa = base
                    .checked_add(u64::from(r.get(0x2144)))
                    .ok_or_else(|| Error("shader address overflow".into()))?;
                let (vs, vh, vkey) = self.shader(va, memory, Stage::Vertex)?;
                let (fs, fh, fkey) = self.shader(fa, memory, Stage::Fragment)?;
                require(
                    fh.color_masks[..attachment_count as usize]
                        .iter()
                        .any(|x| *x != 0)
                        && fh.color_masks[attachment_count as usize..]
                            .iter()
                            .all(|x| *x == 0),
                    "fragment output target mismatch",
                )?;
                let vfile = self.file(vs.source.as_bytes().to_vec());
                let ffile = self.file(fs.source.as_bytes().to_vec());
                let mut cache_state = Vec::new();
                // Canonical descriptor state: absent registers and explicit zero agree.
                // Addresses, draw arguments, viewports and clears are dynamic state.
                for a in (0..=0x2000).step_by(4) {
                    let descriptor = matches!(a, 0x810 | 0x850 | 0x890 | 0x8d0 | 0x910 | 0x950 | 0x990 | 0x9d0 | 0x121c | 0x1e00..=0x1eff | 0xfe8 | 0x1538 | 0xf54..=0xf5c | 0x1160..=0x11dc | 0x12cc | 0x12e4 | 0x12e8 | 0x130c | 0x133c..=0x139c | 0x1594..=0x15a4 | 0x1880..=0x18fc)
                        || ((0x1c00..=0x1dff).contains(&a) && matches!(a % 16, 0 | 12));
                    if descriptor {
                        cache_state.extend((a as u32).to_le_bytes());
                        cache_state.extend(r.get(a as u32).to_le_bytes());
                    }
                }
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
                let mut constants = Vec::new();
                for (stage, shader, group_reg) in [(0, &vs, 0x2050), (1, &fs, 0x2150)] {
                    let group = r.get(group_reg) as usize;
                    require(group < 5, "constant binding group out of range")?;
                    for slot in &shader.constant_buffers {
                        let cb = s.constants[group][*slot as usize]
                            .ok_or_else(|| Error(format!("unbound cbuf {stage}:{slot}")))?;
                        require(cb.size > 0, "empty constant buffer")?;
                        let f = self.file(memory.read(cb.address, cb.size as usize)?.to_vec());
                        constants.push(format!(
                            "{{\"stage\":{stage},\"slot\":{slot},\"file\":{f}}}"
                        ));
                    }
                }
                let base_vertex = r.get(0x1434) as i32;
                let base_instance = r
                    .get(0x1438)
                    .checked_add(instance)
                    .ok_or_else(|| Error("base instance overflow".into()))?;
                let (index_file, min_vertex, max_vertex, index_size) = if indexed {
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
                    let bytes = memory.read(address, (u64::from(count) * size) as usize)?;
                    let indices = bytes
                        .chunks(size as usize)
                        .map(|b| {
                            let mut word = [0; 4];
                            word[..b.len()].copy_from_slice(b);
                            u32::from_le_bytes(word)
                        })
                        .collect::<Vec<_>>();
                    let min = i64::from(*indices.iter().min().unwrap()) + i64::from(base_vertex);
                    let max = i64::from(*indices.iter().max().unwrap()) + i64::from(base_vertex);
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
                        "null".into(),
                        first,
                        first
                            .checked_add(count - 1)
                            .ok_or_else(|| Error("vertex count overflow".into()))?,
                        0,
                    )
                };
                let _ = min_vertex;
                let slots = vh
                    .inputs
                    .iter()
                    .map(|a| (a.address / 16 - 8) as u32)
                    .collect::<BTreeSet<_>>();
                let mut attrs = Vec::new();
                let mut streams = BTreeMap::<u32, u32>::new();
                for slot in slots {
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
                    require(
                        !vs.constant_buffers.contains(&(stream as u8)) && stream < 30,
                        "vertex stream/cbuf/texture buffer collision",
                    )?;
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
                    attrs.push(format!("{{\"slot\":{slot},\"stream\":{stream},\"offset\":{offset},\"format\":{format},\"stride\":{stride},\"instanced\":{instanced},\"rate\":{rate}}}"));
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
                    let file = self.file(memory.read(address, length as usize)?.to_vec());
                    buffers.push(format!("{{\"slot\":{stream},\"file\":{file}}}"));
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
                            b.texture_type == soju_switch_shader::texture::TextureType::D2
                        }),
                        "only 2D texture argument tables supported",
                    )?;
                    require(
                        !shader.texture_bindings.is_empty(),
                        "bindless resources require host-normalized bindings",
                    )?;
                    let mut images = BTreeSet::new();
                    let mut samplers = BTreeSet::new();
                    for b in &shader.texture_bindings {
                        images.insert(b.texture_index);
                        samplers.insert(b.sampler_index);
                    }
                    for index in images {
                        require(index <= r.get(0x157c), "TIC pool index out of bounds")?;
                        let addr = r
                            .address(0x1574)
                            .checked_add(u64::from(index) * 32)
                            .ok_or_else(|| Error("TIC address overflow".into()))?;
                        let tic = Tic::parse(memory.read(addr, 32)?)?;
                        let texture_end = tic
                            .address
                            .checked_add(tic.surface().size() as u64)
                            .ok_or_else(|| Error("texture range overflow".into()))?;
                        for rendered in self.targets.values().chain(batch_targets.values()) {
                            require(
                                tic.address >= rendered.address + rendered.size() as u64
                                    || rendered.address >= texture_end,
                                "render-to-texture feedback requires execute before prepare",
                            )?;
                        }
                        let data = self.textures.upload(&tic, memory)?;
                        let key = Key::new(Inputs {
                            program: &data,
                            header: &tic
                                .raw
                                .into_iter()
                                .flat_map(u32::to_le_bytes)
                                .collect::<Vec<_>>(),
                            render_state: &[],
                        });
                        let key = key.0.iter().map(|v| format!("{v:02x}")).collect::<String>();
                        let file = self.file(data);
                        textures.push(format!("{{\"stage\":{stage},\"count\":{count},\"image\":{index},\"key\":{},\"width\":{},\"height\":{},\"bc1\":{},\"file\":{file}}}",quote(&key),tic.width,tic.height,tic.bc1));
                    }
                    for index in samplers {
                        require(index <= r.get(0x1564), "TSC pool index out of bounds")?;
                        let addr = r
                            .address(0x155c)
                            .checked_add(u64::from(index) * 32)
                            .ok_or_else(|| Error("TSC address overflow".into()))?;
                        let t = Tsc::parse(memory.read(addr, 32)?)?;
                        textures.push(format!("{{\"stage\":{stage},\"count\":{count},\"sampler\":{index},\"u\":{},\"v\":{},\"min\":{},\"mag\":{}}}",t.address_u,t.address_v,t.min_filter,t.mag_filter));
                    }
                }
                write!(command,",\"pipeline\":{},\"vertex\":{vfile},\"fragment\":{ffile},\"attributes\":[{}],\"buffers\":[{}],\"constants\":[{}],\"textures\":[{}],\"topology\":{topology},\"first\":{first},\"count\":{count},\"indices\":{index_file},\"indexSize\":{index_size},\"baseVertex\":{base_vertex},\"instance\":{base_instance}",quote(&pipeline),attrs.join(","),buffers.join(","),constants.join(","),textures.join(",")).unwrap();
            }
        }
        command.push('}');
        self.commands.push(command);
        self.targets.extend(batch_targets);
        Ok(())
    }
    /// Queue a captured batch on the persistent GPU thread. No image readback occurs.
    /// The caller must synchronize before CPU/transfer access to pending targets.
    pub fn submit(&mut self, directory: &Path) -> Result<crate::worker::Fence> {
        require(!self.commands.is_empty(), "empty command batch")?;
        self.serial += 1;
        let directory = directory.join(format!("batch-{}", self.serial));
        std::fs::create_dir_all(&directory).map_err(io)?;
        let directory = std::fs::canonicalize(directory).map_err(io)?;
        require(
            !directory.to_string_lossy().contains(['\n', '\t']),
            "invalid worker directory",
        )?;
        for (i, data) in self.files.iter().enumerate() {
            std::fs::write(directory.join(format!("resource-{i}.bin")), data).map_err(io)?;
        }
        std::fs::write(
            directory.join("commands.json"),
            format!("[{}]", self.commands.join(",")),
        )
        .map_err(io)?;
        let fence = self
            .worker
            .get_or_insert_with(crate::worker::Worker::new)
            .request("submit", directory)?;
        self.commands.clear();
        self.files.clear();
        Ok(fence)
    }
    /// Explicit CPU visibility boundary. Waits for queued work and writes pending
    /// color targets to guest memory. Fence waits alone never copy image bytes.
    pub fn readback(&mut self, directory: &Path, memory: &mut Memory) -> Result<Report> {
        require(
            self.commands.is_empty(),
            "submit prepared commands before readback",
        )?;
        for t in self.targets.values() {
            require(
                memory.identity == t.memory_identity
                    && memory.generation(t.address, t.size())? == t.generation,
                "CPU write overlaps pending target; synchronize before writing",
            )?;
        }
        std::fs::create_dir_all(directory).map_err(io)?;
        let directory = std::fs::canonicalize(directory).map_err(io)?;
        require(
            !directory.to_string_lossy().contains(['\n', '\t']),
            "invalid worker directory",
        )?;
        let diagnostics = self
            .worker
            .as_ref()
            .ok_or_else(|| Error("no submitted work".into()))?
            .request("readback", directory.clone())?
            .wait()?;
        let mut images = Vec::new();
        let mut writes = Vec::new();
        for (key, t) in &self.targets {
            let bytes = std::fs::read(directory.join(format!("{key}.rgba"))).map_err(io)?;
            require(
                bytes.len() == t.width as usize * t.height as usize * 4,
                "invalid readback length",
            )?;
            let data = if t.pitch {
                bytes.clone()
            } else {
                t.surface().swizzle(&bytes).map_err(io)?
            };
            memory.read(t.address, data.len())?;
            writes.push((t.address, data));
            images.push(Readback {
                address: t.address,
                width: t.width,
                height: t.height,
                rgba: bytes,
            });
        }
        for (address, data) in writes {
            memory.write(address, &data)?;
        }
        self.commands.clear();
        self.files.clear();
        self.targets.clear();
        Ok(Report {
            images,
            diagnostics,
        })
    }
    /// Compatibility wrapper: submit, wait, then make color targets CPU-visible.
    pub fn execute(&mut self, directory: &Path, memory: &mut Memory) -> Result<Report> {
        let diagnostics = self.submit(directory)?.wait()?;
        let mut report = self.readback(directory, memory)?;
        report.diagnostics = diagnostics;
        Ok(report)
    }
}
