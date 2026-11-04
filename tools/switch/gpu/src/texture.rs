//! Maxwell block-linear TIC and TSC fields: NVIDIA clb197tex.h (see SOURCES.md).
use crate::{memory::Memory, require, Error, Result};
use soju_switch_texture::Surface;
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
        let ty = (w[0] >> 7) & 7;
        require(
            (0..4).all(|c| (w[0] >> (7 + c * 3)) & 7 == ty),
            "mixed TIC component types unsupported",
        )?;
        let format = crate::format::texture(w[0] & 127, ty, w[4] & (1 << 22) != 0)?;
        require(
            w[0] >> 31 == 0 && (0..4).all(|c| matches!((w[0] >> (19 + c * 3)) & 7, 0 | 2..=7)),
            "invalid TIC swizzle",
        )?;
        require(
            (0..4).all(|c| match (w[0] >> (19 + c * 3)) & 7 {
                6 => ty == 4,
                7 => ty != 4,
                _ => true,
            }),
            "TIC constant-one type mismatch",
        )?;
        let bc1 = matches!(
            format,
            soju_switch_texture::format::Format::Bc1RgbaUnorm
                | soju_switch_texture::format::Format::Bc1RgbaSrgb
        );
        require(
            w[1] & 511 == 0 && w[2] & !0x60ffff == 0,
            "unsupported TIC address fields",
        )?;
        require(
            w[3] & !0x08070038 == 0 && (w[3] >> 3) & 7 <= 5,
            &format!("TIC mips/depth/tile mode unsupported: {w:08x?}"),
        )?;
        let mut w4 = w[4];
        if w4 >> 29 == 7 {
            w4 &= !(7 << 29);
        }
        require(
            w4 & !0x1840ffff == 1 << 23
                && w[5] & !0xffff == 1 << 31
                && matches!(w[6], 0 | 0x03000000)
                && w[7] == 0,
            &format!("TIC requires normalized 2D single mip: {w:08x?}"),
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
    pub fn format(&self) -> soju_switch_texture::format::Format {
        crate::format::texture(
            self.raw[0] & 127,
            (self.raw[0] >> 7) & 7,
            self.raw[4] & (1 << 22) != 0,
        )
        .expect("validated TIC")
    }
    pub fn swizzle(&self) -> [u32; 4] {
        std::array::from_fn(|c| match (self.raw[0] >> (19 + c * 3)) & 7 {
            6 | 7 => 1,
            v => v,
        })
    }
    pub fn surface(&self) -> Surface {
        let info = self.format().info();
        Surface {
            width: (self.width as usize).div_ceil(info.block_w),
            height: (self.height as usize).div_ceil(info.block_h),
            depth: 1,
            bytes_per_block: info.bytes,
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
        // The comparison function is inactive when DEPTH_COMPARE (bit 9) is zero.
        require(
            u <= 2 && v <= 2 && p <= 2 && matches!(w[0] & !0x1dff, 0 | 0x26000),
            &format!("TSC wrap/compare/aniso unsupported: control={:#010x}", w[0]),
        )?;
        let mag = w[1] & 7;
        let min = (w[1] >> 4) & 3;
        require(
            matches!(mag, 1 | 2)
                && matches!(min, 1 | 2)
                && matches!(w[1] & !0x37, 0x40 | 0x340)
                && matches!(w[2], 0 | 0x00f00000)
                && w[3..].iter().all(|x| *x == 0),
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
    memory_identity: Option<u64>,
    entries: crate::lru::Lru<[u32; 8], (u64, Vec<u8>)>,
    pub uploads: u64,
    pub hits: u64,
}
impl TextureCache {
    pub fn upload(&mut self, tic: &Tic, memory: &Memory) -> Result<Vec<u8>> {
        if self.memory_identity != Some(memory.identity) {
            self.entries.clear();
            self.memory_identity = Some(memory.identity);
        }
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
        self.entries
            .insert(tic.raw, (generation, data.clone()), data.len() + 64);
        self.uploads += 1;
        Ok(data)
    }
}
