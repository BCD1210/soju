//! Switch GPU texture layouts, written from public descriptions of the Tegra X1
//! block-linear format. No emulator code is used.
//!
//! Block-linear memory is made of GOBs (64 bytes wide x 8 rows = 512 bytes). GOBs are
//! stacked `block_height` high (and `block_depth` deep) into blocks; blocks are laid
//! out left to right, then top to bottom. Inside a GOB, 16-byte pieces follow
//! `((x % 64) / 32) * 256 + ((y % 8) / 2) * 64 + ((x % 32) / 16) * 32 + (y % 2) * 16 + x % 16`.

pub mod format;

pub const GOB_W: usize = 64;
pub const GOB_H: usize = 8;
pub const GOB_SIZE: usize = GOB_W * GOB_H;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    TooLarge,
    BadParameter(&'static str),
    SourceTooSmall { needed: usize, got: usize },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::TooLarge => write!(f, "texture is too large"),
            Error::BadParameter(m) => write!(f, "bad texture parameter: {m}"),
            Error::SourceTooSmall { needed, got } => write!(f, "texture data is {got} bytes, {needed} needed"),
        }
    }
}

impl std::error::Error for Error {}

pub const MAX_DIM: usize = 16384;

/// One mip level / surface in block-linear layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Surface {
    /// Size in format blocks (pixels for uncompressed formats, 4x4 blocks for BCn...).
    pub width: usize,
    pub height: usize,
    pub depth: usize,
    pub bytes_per_block: usize,
    /// GOBs per block vertically / in depth, as log2 (0..=5).
    pub block_height_log2: u32,
    pub block_depth_log2: u32,
}

fn div_up(a: usize, b: usize) -> usize {
    a.div_ceil(b)
}

impl Surface {
    /// Validate dimensions and block geometry before allocating or borrowing storage.
    pub fn check(&self) -> Result<(), Error> {
        if self.width == 0 || self.height == 0 || self.depth == 0 {
            return Err(Error::BadParameter("zero size"));
        }
        if self.width > MAX_DIM || self.height > MAX_DIM || self.depth > 2048 {
            return Err(Error::TooLarge);
        }
        if !matches!(self.bytes_per_block, 1 | 2 | 4 | 8 | 12 | 16) {
            return Err(Error::BadParameter("bytes per block"));
        }
        if self.block_height_log2 > 5 || self.block_depth_log2 > 5 {
            return Err(Error::BadParameter("block height/depth"));
        }
        Ok(())
    }

    pub fn gob_block_height(&self) -> usize {
        1 << self.block_height_log2
    }

    pub fn gob_block_depth(&self) -> usize {
        1 << self.block_depth_log2
    }

    /// Width of one row in bytes, rounded up to whole GOBs.
    pub fn row_bytes_aligned(&self) -> usize {
        div_up(self.width * self.bytes_per_block, GOB_W) * GOB_W
    }

    /// Size in bytes of this surface in block-linear memory.
    pub fn size(&self) -> usize {
        let block_rows = GOB_H * self.gob_block_height();
        let blocks_x = div_up(self.width * self.bytes_per_block, GOB_W);
        let blocks_y = div_up(self.height, block_rows);
        let blocks_z = div_up(self.depth, self.gob_block_depth());
        blocks_x * blocks_y * blocks_z * GOB_SIZE * self.gob_block_height() * self.gob_block_depth()
    }

    /// Byte offset in block-linear memory of byte `x_bytes` of row `y` in slice `z`.
    #[inline]
    pub fn offset(&self, x_bytes: usize, y: usize, z: usize) -> usize {
        let bh = self.gob_block_height();
        let bd = self.gob_block_depth();
        let block_bytes = GOB_SIZE * bh * bd;
        let blocks_x = div_up(self.width * self.bytes_per_block, GOB_W);
        let blocks_y = div_up(self.height, GOB_H * bh);
        let block = (z / bd) * blocks_x * blocks_y + (y / (GOB_H * bh)) * blocks_x + x_bytes / GOB_W;
        let gob_in_block = (z % bd) * bh + (y / GOB_H) % bh;
        block * block_bytes
            + gob_in_block * GOB_SIZE
            + ((x_bytes % 64) / 32) * 256
            + ((y % 8) / 2) * 64
            + ((x_bytes % 32) / 16) * 32
            + (y % 2) * 16
            + x_bytes % 16
    }

    /// Block-linear -> linear (tightly packed rows).
    pub fn deswizzle(&self, src: &[u8]) -> Result<Vec<u8>, Error> {
        self.check()?;
        let need = self.size();
        if src.len() < need {
            return Err(Error::SourceTooSmall { needed: need, got: src.len() });
        }
        let row = self.width * self.bytes_per_block;
        let mut out = vec![0u8; row * self.height * self.depth];
        let columns: Vec<_> = (0..row / 16).map(|x| self.offset(x * 16, 0, 0)).collect();
        for z in 0..self.depth {
            for y in 0..self.height {
                let base = self.offset(0, y, z);
                let dst = &mut out[(z * self.height + y) * row..][..row];
                for (piece, &column) in dst.chunks_exact_mut(16).zip(&columns) {
                    piece.copy_from_slice(&src[base + column..][..16]);
                }
                let tail = row % 16;
                if tail != 0 {
                    let x = row - tail;
                    let o = self.offset(x, y, z);
                    dst[x..].copy_from_slice(&src[o..o + tail]);
                }
            }
        }
        Ok(out)
    }

    /// Linear -> block-linear (padding bytes are zero).
    pub fn swizzle(&self, linear: &[u8]) -> Result<Vec<u8>, Error> {
        self.check()?;
        let needed = self.width * self.height * self.depth * self.bytes_per_block;
        if linear.len() < needed {
            return Err(Error::SourceTooSmall { needed, got: linear.len() });
        }
        let mut out = vec![0u8; self.size()];
        self.swizzle_into(linear, &mut out)?;
        Ok(out)
    }

    /// Copy active texels into existing storage, preserving all layout padding.
    pub fn swizzle_into(&self, linear: &[u8], out: &mut [u8]) -> Result<(), Error> {
        self.check()?;
        let row = self.width * self.bytes_per_block;
        let need = row * self.height * self.depth;
        if linear.len() < need {
            return Err(Error::SourceTooSmall { needed: need, got: linear.len() });
        }
        if out.len() < self.size() {
            return Err(Error::SourceTooSmall { needed: self.size(), got: out.len() });
        }
        // X and Y/Z address contributions are independent at 16-byte boundaries.
        // Fixed-size pieces avoid a variable-length copy for every pixel group.
        let columns: Vec<_> = (0..row / 16).map(|x| self.offset(x * 16, 0, 0)).collect();
        for z in 0..self.depth {
            for y in 0..self.height {
                let base = self.offset(0, y, z);
                let src = &linear[(z * self.height + y) * row..][..row];
                for (piece, &column) in src.chunks_exact(16).zip(&columns) {
                    out[base + column..][..16].copy_from_slice(piece);
                }
                let tail = row % 16;
                if tail != 0 {
                    let x = row - tail;
                    let o = self.offset(x, y, z);
                    out[o..o + tail].copy_from_slice(&src[x..]);
                }
            }
        }
        Ok(())
    }
}

/// A full texture: mip chain and array layers, as the GPU lays them out.
#[derive(Debug, Clone, Copy)]
pub struct Texture {
    pub width: usize,
    pub height: usize,
    pub depth: usize,
    pub layers: usize,
    pub levels: usize,
    pub format: format::Format,
    pub block_height_log2: u32,
    pub block_depth_log2: u32,
}

impl Texture {
    /// Mip level `level` as a Surface. The GOB block height/depth shrink while the
    /// level is at most half a block tall/deep, so small mips do not waste memory.
    pub fn level(&self, level: usize) -> Surface {
        let info = self.format.info();
        let w = (self.width >> level).max(1);
        let h = (self.height >> level).max(1);
        let d = (self.depth >> level).max(1);
        let bw = div_up(w, info.block_w);
        let bh_blocks = div_up(h, info.block_h);
        let mut bh = self.block_height_log2;
        while bh > 0 && bh_blocks <= (GOB_H << bh) / 2 {
            bh -= 1;
        }
        let mut bd = self.block_depth_log2;
        while bd > 0 && d <= (1 << bd) / 2 {
            bd -= 1;
        }
        Surface { width: bw, height: bh_blocks, depth: d, bytes_per_block: info.bytes, block_height_log2: bh, block_depth_log2: bd }
    }

    /// Bytes of one array layer (all mips), aligned to the base level's block size.
    pub fn layer_size(&self) -> usize {
        let total: usize = (0..self.levels).map(|l| self.level(l).size()).sum();
        let base = self.level(0);
        let align = GOB_SIZE * base.gob_block_height() * base.gob_block_depth();
        if self.layers > 1 { div_up(total, align) * align } else { total }
    }

    /// Offset of (layer, level) from the start of the texture.
    pub fn offset(&self, layer: usize, level: usize) -> usize {
        layer * self.layer_size() + (0..level).map(|l| self.level(l).size()).sum::<usize>()
    }

    pub fn size(&self) -> usize {
        self.layer_size() * self.layers
    }

    /// Deswizzle every layer and mip into one packed buffer (layer-major, then level).
    pub fn deswizzle_all(&self, src: &[u8]) -> Result<Vec<Vec<u8>>, Error> {
        if self.levels == 0 || self.levels > 16 || self.layers == 0 || self.layers > 2048 {
            return Err(Error::BadParameter("levels/layers"));
        }
        if src.len() < self.size() {
            return Err(Error::SourceTooSmall { needed: self.size(), got: src.len() });
        }
        let mut out = Vec::with_capacity(self.layers * self.levels);
        for layer in 0..self.layers {
            for level in 0..self.levels {
                let o = self.offset(layer, level);
                out.push(self.level(level).deswizzle(&src[o..])?);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use format::Format;

    /// The same arithmetic as tools/switch/frame_convert.c (the runner's existing RGBA path).
    fn reference_rgba(src: &[u8], width: usize, height: usize, pitch: usize, log2_h: u32) -> Vec<u8> {
        let bh = 1usize << log2_h;
        let block_rows = 8 * bh;
        let mut dst = vec![0u8; width * height * 4];
        for y in 0..height {
            let base = (y / block_rows) * pitch * block_rows + ((y / 8) % bh) * 512 + ((y % 8) / 2) * 64 + (y % 2) * 16;
            let mut x = 0;
            while x < width * 4 {
                let off = base + (x / 64) * 512 * bh + ((x % 64) / 32) * 256 + ((x % 32) / 16) * 32;
                let n = (width * 4 - x).min(16);
                dst[y * width * 4 + x..y * width * 4 + x + n].copy_from_slice(&src[off..off + n]);
                x += 16;
            }
        }
        dst
    }

    fn noise(n: usize, seed: u32) -> Vec<u8> {
        let mut s = seed;
        (0..n).map(|_| { s = s.wrapping_mul(1664525).wrapping_add(1013904223); (s >> 24) as u8 }).collect()
    }

    #[test]
    fn matches_existing_runner_for_framebuffers() {
        for (w, h, lh) in [(1280usize, 720usize, 4u32), (16, 8, 0), (100, 37, 2), (64, 128, 5)] {
            let s = Surface { width: w, height: h, depth: 1, bytes_per_block: 4, block_height_log2: lh, block_depth_log2: 0 };
            let src = noise(s.size(), 7);
            assert_eq!(s.deswizzle(&src).unwrap(), reference_rgba(&src, w, h, s.row_bytes_aligned(), lh), "{w}x{h} bh{lh}");
        }
    }

    #[test]
    fn swizzle_round_trip_all_sizes() {
        for bpb in [1, 2, 4, 8, 12, 16] {
            for (w, h, d, lh, ld) in [(1, 1, 1, 0, 0), (33, 17, 1, 3, 0), (300, 5, 1, 4, 0), (20, 20, 9, 1, 2)] {
                let s = Surface { width: w, height: h, depth: d, bytes_per_block: bpb, block_height_log2: lh, block_depth_log2: ld };
                let lin = noise(w * h * d * bpb, (w * 31 + bpb) as u32);
                let bl = s.swizzle(&lin).unwrap();
                assert_eq!(bl.len(), s.size());
                assert_eq!(s.deswizzle(&bl).unwrap(), lin, "bpb {bpb} {w}x{h}x{d}");
            }
        }
    }

    #[test]
    fn gob_layout_basics() {
        let s = Surface { width: 16, height: 8, depth: 1, bytes_per_block: 4, block_height_log2: 0, block_depth_log2: 0 };
        assert_eq!(s.size(), 512);
        assert_eq!(s.offset(0, 1, 0), 16);
        assert_eq!(s.offset(16, 0, 0), 32);
        assert_eq!(s.offset(32, 0, 0), 256);
        assert_eq!(s.offset(0, 2, 0), 64);
    }

    #[test]
    fn compressed_mip_chain_and_layers() {
        let t = Texture { width: 256, height: 256, depth: 1, layers: 2, levels: 9, format: Format::Bc1RgbaUnorm, block_height_log2: 4, block_depth_log2: 0 };
        let l0 = t.level(0);
        assert_eq!((l0.width, l0.height, l0.bytes_per_block), (64, 64, 8));
        // 64 block rows need 8 GOBs: block height shrinks from 16 to 8.
        assert_eq!(l0.block_height_log2, 3);
        let last = t.level(8);
        assert_eq!((last.width, last.height, last.block_height_log2), (1, 1, 0));
        assert_eq!(t.offset(1, 0) % (GOB_SIZE * 8), 0);
        let src = noise(t.size(), 3);
        let all = t.deswizzle_all(&src).unwrap();
        assert_eq!(all.len(), 18);
        assert_eq!(all[0].len(), 64 * 64 * 8);
        assert_eq!(all[17].len(), 8);
    }

    #[test]
    fn swizzle_into_preserves_padding_and_matches_byte_addressing() {
        for bh in 0..=5 {
            for bd in 0..=2 {
                for bytes in [1, 2, 4, 8, 12, 16] {
                    for width in [1, 3, 17, 65] {
                        let surface = Surface { width, height: 19, depth: 3, bytes_per_block: bytes,
                            block_height_log2: bh, block_depth_log2: bd };
                        let row = width * bytes;
                        let linear = noise(row * 19 * 3, 71);
                        let mut expected = vec![0xa7; surface.size()];
                        for z in 0..3 { for y in 0..19 { for x in 0..row {
                            expected[surface.offset(x, y, z)] = linear[(z * 19 + y) * row + x];
                        }}}
                        let mut actual = vec![0xa7; surface.size()];
                        surface.swizzle_into(&linear, &mut actual).unwrap();
                        assert_eq!(actual, expected);
                        assert_eq!(surface.deswizzle(&expected).unwrap(), linear);
                        let before = actual.clone();
                        assert!(surface.swizzle_into(&linear[..linear.len()-1], &mut actual).is_err());
                        assert_eq!(actual, before);
                    }
                }
            }
        }
    }

    #[test]
    fn rejects_bad_input() {
        let s = Surface { width: 8, height: 8, depth: 1, bytes_per_block: 3, block_height_log2: 0, block_depth_log2: 0 };
        assert!(s.deswizzle(&[0; 4096]).is_err());
        let s = Surface { bytes_per_block: 4, ..s };
        assert!(matches!(s.deswizzle(&[0; 10]), Err(Error::SourceTooSmall { .. })));
        let big = Surface { width: MAX_DIM + 1, ..s };
        assert!(matches!(big.deswizzle(&[]), Err(Error::TooLarge)));
    }
}
