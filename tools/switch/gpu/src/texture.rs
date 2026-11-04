//! Maxwell block-linear TIC and TSC fields: NVIDIA clb197tex.h (see SOURCES.md).
use crate::{memory::Memory, require, Error, Result};
use soju_switch_texture::Surface;
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tic {
    pub raw: [u32; 8],
    pub address: u64,
    pub width: u32,
    pub height: u32,
    pub bc1: bool,
    pub block_height: u32,
}
fn words(bytes: &[u8]) -> Result<[u32; 8]> {
    require(bytes.len() == 32, "descriptor must be 32 bytes")?;
    Ok(std::array::from_fn(|i| {
        u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap())
    }))
}
impl Tic {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let w = words(bytes)?;
        require(
            (w[2] >> 21) & 7 == 3,
            "only Maxwell block-linear TIC supported",
        )?;
        let bc1 = match w[0] & 127 {
            8 => false,
            0x24 => true,
            _ => return Err(Error("TIC format unsupported".into())),
        };
        let identity = (2 << 7)
            | (2 << 10)
            | (2 << 13)
            | (2 << 16)
            | (2 << 19)
            | (3 << 22)
            | (4 << 25)
            | (5 << 28);
        require(
            w[0] & !127 == identity,
            "TIC requires UNORM identity components",
        )?;
        require(
            w[1] & 511 == 0 && w[2] & !0x60ffff == 0,
            "unsupported TIC address fields",
        )?;
        require(
            w[3] & !0x38 == 0 && (w[3] >> 3) & 7 <= 5,
            "TIC mips/depth/tile mode unsupported",
        )?;
        require(
            w[4] & !0xffff == 1 << 23 && w[5] & !0xffff == 1 << 31 && w[6] == 0 && w[7] == 0,
            "TIC requires normalized 2D single mip, no sRGB",
        )?;
        let width = (w[4] & 65535) + 1;
        let height = (w[5] & 65535) + 1;
        require(
            width <= 16384 && height <= 16384,
            "TIC dimensions exceed limit",
        )?;
        Ok(Self {
            raw: w,
            address: u64::from(w[1]) | (u64::from(w[2] & 65535) << 32),
            width,
            height,
            bc1,
            block_height: (w[3] >> 3) & 7,
        })
    }
    pub fn surface(&self) -> Surface {
        Surface {
            width: if self.bc1 {
                self.width.div_ceil(4)
            } else {
                self.width
            } as usize,
            height: if self.bc1 {
                self.height.div_ceil(4)
            } else {
                self.height
            } as usize,
            depth: 1,
            bytes_per_block: if self.bc1 { 8 } else { 4 },
            block_height_log2: self.block_height,
            block_depth_log2: 0,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Tsc {
    pub address_u: u32,
    pub address_v: u32,
    pub min_filter: u32,
    pub mag_filter: u32,
}
impl Tsc {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let w = words(bytes)?;
        let (u, v, p) = (w[0] & 7, (w[0] >> 3) & 7, (w[0] >> 6) & 7);
        require(
            u <= 2 && v <= 2 && p <= 2 && w[0] & !511 == 0,
            "TSC wrap/compare/aniso unsupported",
        )?;
        let mag = w[1] & 7;
        let min = (w[1] >> 4) & 3;
        require(
            matches!(mag, 1 | 2)
                && matches!(min, 1 | 2)
                && w[1] & !0x37 == 0x40
                && w[2..].iter().all(|x| *x == 0),
            "TSC requires point/linear, no mip/LOD/border overrides",
        )?;
        Ok(Self {
            address_u: u,
            address_v: v,
            min_filter: min,
            mag_filter: mag,
        })
    }
}
#[derive(Default)]
pub struct TextureCache {
    entries: BTreeMap<[u32; 8], (u64, Vec<u8>)>,
    pub uploads: u64,
    pub hits: u64,
}
impl TextureCache {
    pub fn upload(&mut self, tic: &Tic, memory: &Memory) -> Result<Vec<u8>> {
        let surface = tic.surface();
        let generation = memory.generation(tic.address, surface.size())?;
        if let Some((old, bytes)) = self.entries.get(&tic.raw) {
            if *old == generation {
                self.hits += 1;
                return Ok(bytes.clone());
            }
        }
        let data = surface
            .deswizzle(memory.read(tic.address, surface.size())?)
            .map_err(|e| Error(e.to_string()))?;
        self.entries.insert(tic.raw, (generation, data.clone()));
        self.uploads += 1;
        Ok(data)
    }
}
