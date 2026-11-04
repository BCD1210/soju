//! B1C0 launch methods and standalone QMD grids. See SOURCES.md and REGISTERS.md.
use crate::{engine::ConstantBuffer, memory::Memory, require, Error, Result};
use soju_switch_shader::compute::Launch;
pub const B1C0: u32 = 0xb1c0;

#[derive(Clone, Debug)]
pub struct Dispatch {
    pub(crate) qmd_address: u64,
    pub(crate) launch: Launch,
    pub(crate) program: u64,
    pub(crate) groups: [u32; 3],
    pub(crate) constants: [Option<ConstantBuffer>; 8],
}
impl Dispatch {
    pub fn launch(&self) -> &Launch {
        &self.launch
    }
    pub fn program(&self) -> u64 {
        self.program
    }
    pub fn groups(&self) -> [u32; 3] {
        self.groups
    }
    pub fn constant(&self, slot: usize) -> Option<ConstantBuffer> {
        self.constants.get(slot).copied().flatten()
    }
    fn parse(bytes: &[u8], base: u64, qmd_address: u64) -> Result<Self> {
        let launch = Launch::parse(bytes).map_err(|e| Error(e.to_string()))?;
        let raw = &launch.raw;
        // Reject scheduling, queue, semaphore, resume and numerical modes rather
        // than quietly executing a different grid. Cache invalidation is implicit
        // in Memory range generations; cache allocation hints have no semantic effect.
        for (i, &word) in raw.iter().enumerate() {
            let allowed = match i {
                6 => (1 << 12) | (1 << 6), // scheduling PCAS and global cache hint
                7 => 0xfc00_0000,          // cache invalidation flags
                8 | 12 | 13 | 19 => u32::MAX,
                11 => 1 << 26, // unlimited API calls; CALL is rejected by translator
                17 => 0x3ffff,
                18 => 0xffff_00ff,
                20 => 0x6000_00ff, // cbuf valid and L1 configuration
                29..=44 if i % 2 == 1 => u32::MAX,
                29..=44 => 0xffff_c0ff, // size, invalidate, 40-bit address
                45 | 47 => 0x00ff_ffff,
                46 => u32::MAX, // local negative and register count
                _ => 0,
            };
            require(
                word & !allowed == 0,
                &format!("unsupported QMD word {i} flags"),
            )?;
        }
        require(
            raw[47] <= 1024 * 1024,
            "compute call stack allocation exceeds limit",
        )?;
        let groups = [raw[12], raw[13] & 0xffff, raw[13] >> 16];
        require(groups.iter().all(|v| *v > 0), "empty compute grid")?;
        require(
            groups
                .into_iter()
                .try_fold(1u64, |n, v| n.checked_mul(u64::from(v)))
                .is_some_and(|n| n <= 16_777_216),
            "compute grid exceeds backend limit",
        )?;
        let program = base
            .checked_add(u64::from(raw[8]))
            .ok_or_else(|| Error("compute program address overflow".into()))?;
        let mut constants = [None; 8];
        for (slot, cb) in constants.iter_mut().enumerate() {
            if raw[20] & (1 << slot) != 0 {
                let lower = raw[29 + slot * 2];
                let upper = raw[30 + slot * 2];
                let address = u64::from(lower) | (u64::from(upper & 255) << 32);
                let size = upper >> 15;
                require(
                    address & 3 == 0 && size > 0 && size <= 65536,
                    "compute cbuf alignment/size",
                )?;
                *cb = Some(ConstantBuffer { address, size });
            }
        }
        Ok(Self {
            qmd_address,
            launch,
            program,
            groups,
            constants,
        })
    }
}

#[derive(Default)]
pub struct Engine {
    program: u64,
    qmd: Option<u64>,
}
impl Engine {
    /// Byte method offsets; QMD contents are captured on the scheduling method.
    pub fn method(
        &mut self,
        class: u32,
        offset: u32,
        value: u32,
        memory: &Memory,
    ) -> Result<Option<Dispatch>> {
        require(class == B1C0, "compute engine class must be B1C0")?;
        match offset {
            0x1608 => {
                require(value <= 255, "compute program address upper bits")?;
                self.program = (self.program & 0xffff_ffff) | (u64::from(value) << 32);
            }
            0x160c => self.program = (self.program & !0xffff_ffff) | u64::from(value),
            0x2b4 => self.qmd = Some(u64::from(value) << 8),
            0x2bc => {
                require(value & !3 == 0, "unsupported signaling PCAS bits")?;
                if value & 2 != 0 {
                    let address = self
                        .qmd
                        .ok_or_else(|| Error("compute schedule without QMD address".into()))?;
                    return Ok(Some(Dispatch::parse(
                        memory.read(address, 256)?,
                        self.program,
                        address,
                    )?));
                }
            }
            _ => return Err(Error(format!("unsupported B1C0 method {offset:#x}"))),
        }
        Ok(None)
    }
}
