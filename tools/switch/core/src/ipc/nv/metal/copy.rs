//! B0B5 virtual-address copies; field definitions are NVIDIA clb0b5.h.
use super::*;
use soju_switch_texture::Surface;

#[derive(Default)]
pub(super) struct CopyEngine(BTreeMap<u32, u32>);
impl CopyEngine {
    fn get(&self, method: u32) -> u32 {
        self.0.get(&method).copied().unwrap_or(0)
    }
    pub(super) fn method(
        &mut self,
        method: u32,
        value: u32,
        memory: &mut Memory,
    ) -> Result<(), Error> {
        if method != 0x300 {
            if !method.is_multiple_of(4)
                || !matches!(method, 0x400..=0x41c | 0x700..=0x720 | 0x728..=0x73c)
            {
                return Err(Error::Message(format!(
                    "B0B5 unknown method {method:#x}={value:#x}"
                )));
            }
            self.0.insert(method, value);
            return Ok(());
        }
        if value & !0x100f87 != 0 || !matches!(value & 3, 1 | 2) {
            return Err(Error::Message(format!(
                "B0B5 unsupported DMA flags {value:#x}"
            )));
        }
        let unit = if value & 0x400 != 0 {
            let r = self.get(0x708);
            let count = ((r >> 20) & 3) + 1;
            if r & !0x03337777 != 0
                || ((r >> 24) & 3) + 1 != count
                || (0..count).any(|c| (r >> (c * 4)) & 7 != c)
            {
                return Err(Error::Malformed(
                    "B0B5 nonidentity component remap unsupported",
                ));
            }
            (count * (((r >> 16) & 3) + 1)) as usize
        } else {
            1
        };
        let width = self.get(0x418) as usize;
        let rows = if value & 0x200 != 0 {
            self.get(0x41c) as usize
        } else {
            1
        };
        let row_bytes = width
            .checked_mul(unit)
            .ok_or(Error::Malformed("B0B5 row overflow"))?;
        if width == 0
            || rows == 0
            || rows > 16384
            || row_bytes
                .checked_mul(rows)
                .is_none_or(|n| n > 8 * 1024 * 1024)
        {
            return Err(Error::Malformed("B0B5 transfer bounds"));
        }
        let source = self.layout(true, value & 0x80 != 0, unit, width, rows)?;
        let target = self.layout(false, value & 0x100 != 0, unit, width, rows)?;
        for layout in [&source, &target] {
            if layout
                .base
                .checked_add(layout.size as u64)
                .is_none_or(|end| end > 1u64 << 40)
            {
                return Err(Error::Malformed("B0B5 range exceeds 40 bits"));
            }
        }
        let input = memory.read(source.base, source.size).map_err(gpu_error)?;
        let mut output = memory
            .read(target.base, target.size)
            .map_err(gpu_error)?
            .to_vec();
        for y in 0..rows {
            for x in 0..row_bytes {
                output[target.offset(x, y)] = input[source.offset(x, y)];
            }
        }
        // Both full bounds were checked before the only write; overlap uses captured input.
        memory.write(target.base, &output).map_err(gpu_error)
    }
    fn layout(
        &self,
        source: bool,
        pitch: bool,
        unit: usize,
        width: usize,
        rows: usize,
    ) -> Result<Layout, Error> {
        let address_reg = if source { 0x400 } else { 0x408 };
        if self.get(address_reg) > 255 {
            return Err(Error::Malformed("B0B5 address exceeds 40 bits"));
        }
        let base = (u64::from(self.get(address_reg)) << 32) | u64::from(self.get(address_reg + 4));
        if pitch {
            let stride = self.get(if source { 0x410 } else { 0x414 }) as usize;
            let row_bytes = width * unit;
            if rows > 1 && stride < row_bytes {
                return Err(Error::Malformed("B0B5 pitch smaller than row"));
            }
            let size = (rows - 1)
                .checked_mul(stride)
                .and_then(|n| n.checked_add(row_bytes))
                .filter(|n| *n <= 64 * 1024 * 1024)
                .ok_or(Error::Malformed("B0B5 pitched extent"))?;
            return Ok(Layout {
                base,
                size,
                stride,
                origin: (0, 0),
                surface: None,
            });
        }
        let r = if source { 0x728 } else { 0x70c };
        let block = self.get(r);
        let (w, h) = (self.get(r + 4) as usize, self.get(r + 8) as usize);
        let origin = self.get(r + 20);
        let (x, y) = ((origin & 65535) as usize, (origin >> 16) as usize);
        if block & !0x1070 != 0
            || block & 0x1000 == 0
            || (block >> 4) & 7 > 5
            || self.get(r + 12) != 1
            || self.get(r + 16) != 0
            || w == 0
            || h == 0
            || w > 16384
            || h > 16384
            || x + width > w
            || y + rows > h
            || !matches!(unit, 1 | 2 | 4 | 8 | 12 | 16)
        {
            return Err(Error::Malformed(
                "B0B5 block-linear extent/layout unsupported",
            ));
        }
        let surface = Surface {
            width: w,
            height: h,
            depth: 1,
            bytes_per_block: unit,
            block_height_log2: (block >> 4) & 7,
            block_depth_log2: 0,
        };
        let size = surface.size();
        if size > 64 * 1024 * 1024 {
            return Err(Error::Malformed("B0B5 surface size limit"));
        }
        Ok(Layout {
            base,
            size,
            stride: 0,
            origin: (x * unit, y),
            surface: Some(surface),
        })
    }
}
struct Layout {
    base: u64,
    size: usize,
    stride: usize,
    origin: (usize, usize),
    surface: Option<Surface>,
}
impl Layout {
    fn offset(&self, x: usize, y: usize) -> usize {
        match self.surface {
            Some(s) => s.offset(self.origin.0 + x, self.origin.1 + y, 0),
            None => y * self.stride + x,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn configured(values: &[(u32, u32)]) -> CopyEngine {
        CopyEngine(values.iter().copied().collect())
    }
    #[test]
    fn pitched_rectangle_preserves_padding_and_snapshots_overlap() {
        let mut memory = Memory::default();
        memory.map(0x1000, (0..64).collect()).unwrap();
        let mut engine = configured(&[
            (0x404, 0x1000),
            (0x40c, 0x1004),
            (0x410, 8),
            (0x414, 8),
            (0x418, 6),
            (0x41c, 3),
        ]);
        let original = memory.read(0x1000, 64).unwrap().to_vec();
        engine.method(0x300, 0x386, &mut memory).unwrap();
        let bytes = memory.read(0x1000, 64).unwrap();
        for y in 0..3 {
            assert_eq!(&bytes[4 + y * 8..10 + y * 8], &original[y * 8..y * 8 + 6]);
        }
        assert_eq!(&bytes[26..], &original[26..]);
        assert_eq!(&bytes[10..12], &original[10..12]);
    }
    #[test]
    fn block_linear_roundtrip_uses_origins_and_preserves_unwritten_texels() {
        let mut memory = Memory::default();
        let input: Vec<u8> = (0..96).collect();
        let surface = Surface {
            width: 11,
            height: 13,
            depth: 1,
            bytes_per_block: 4,
            block_height_log2: 1,
            block_depth_log2: 0,
        };
        memory.map(0x1000, input.clone()).unwrap();
        memory.map(0x10000, vec![0xa5; surface.size()]).unwrap();
        memory.map(0x20000, vec![0; 96]).unwrap();
        let mut engine = configured(&[
            (0x404, 0x1000),
            (0x40c, 0x10000),
            (0x410, 24),
            (0x418, 6),
            (0x41c, 4),
            (0x708, 0x03303210),
            (0x70c, 0x1010),
            (0x710, 11),
            (0x714, 13),
            (0x718, 1),
            (0x720, (3 << 16) | 2),
        ]);
        engine.method(0x300, 0x686, &mut memory).unwrap();
        let bytes = memory.read(0x10000, surface.size()).unwrap();
        for y in 0..13 {
            for x in 0..44 {
                let expected = if (3..7).contains(&y) && (8..32).contains(&x) {
                    input[(y - 3) * 24 + x - 8]
                } else {
                    0xa5
                };
                assert_eq!(bytes[surface.offset(x, y, 0)], expected);
            }
        }
        let mut reverse = configured(&[
            (0x404, 0x10000),
            (0x40c, 0x20000),
            (0x414, 24),
            (0x418, 6),
            (0x41c, 4),
            (0x708, 0x03303210),
            (0x728, 0x1010),
            (0x72c, 11),
            (0x730, 13),
            (0x734, 1),
            (0x73c, (3 << 16) | 2),
        ]);
        reverse.method(0x300, 0x706, &mut memory).unwrap();
        assert_eq!(memory.read(0x20000, 96).unwrap(), input);
    }
    #[test]
    fn invalid_copy_has_no_partial_write() {
        let mut memory = Memory::default();
        memory.map(0x1000, vec![7; 64]).unwrap();
        let mut engine = configured(&[(0x404, 0x1000), (0x40c, 0x103c), (0x418, 8)]);
        assert!(engine.method(0x300, 0x186, &mut memory).is_err());
        assert!(engine.method(0x300, 0x18e, &mut memory).is_err());
        assert!(engine.method(0x999, 1, &mut memory).is_err());
        assert_eq!(memory.read(0x1000, 64).unwrap(), [7; 64]);
    }
}
