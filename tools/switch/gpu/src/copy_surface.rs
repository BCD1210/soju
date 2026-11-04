//! 902D surface addressing from NVIDIA cl902d.h; layout math is shared with textures.
use crate::{Result, engine::Registers, memory::Memory, require};
use soju_switch_texture::Surface;

struct Layout {
    surface: Surface,
    pitch: Option<usize>,
    size: usize,
}
impl Layout {
    fn new(r: &Registers, base: u32) -> Result<Self> {
        let (width, height) = (r.get(base + 0x18) as usize, r.get(base + 0x1c) as usize);
        require(
            width > 0 && height > 0 && width <= 16384 && height <= 16384,
            "902D surface dimensions",
        )?;
        let block = r.get(base + 8);
        require(
            block & !0x70 == 0 && block >> 4 <= 5,
            "902D layered/block layout",
        )?;
        let surface = Surface {
            width,
            height,
            depth: 1,
            bytes_per_block: 4,
            block_height_log2: block >> 4,
            block_depth_log2: 0,
        };
        let pitch = match r.get(base + 4) {
            0 => None,
            1 => {
                let pitch = r.get(base + 0x14) as usize;
                require(pitch >= width * 4, "902D pitch too small")?;
                Some(pitch)
            }
            _ => return Err(crate::Error("902D memory layout".into())),
        };
        let size = pitch.map_or_else(|| surface.size(), |pitch| pitch * height);
        require(
            size <= 64 * 1024 * 1024,
            "902D surface exceeds storage budget",
        )?;
        Ok(Self {
            surface,
            pitch,
            size,
        })
    }
    fn offset(&self, x: usize, y: usize) -> usize {
        self.pitch.map_or_else(
            || self.surface.offset(x * 4, y, 0),
            |pitch| y * pitch + x * 4,
        )
    }
}

pub(crate) fn copy(r: &Registers, memory: &mut Memory, rect: [u32; 6]) -> Result<()> {
    let [dx, dy, w, h, sx, sy] = rect.map(|v| v as usize);
    let src = Layout::new(r, 0x230)?;
    let dst = Layout::new(r, 0x200)?;
    let source = memory.read(r.address(0x250), src.size)?;
    // Capture destination before any write, including when both surfaces alias.
    // Keeping untouched bytes preserves row/GOB padding and the outside rectangle.
    let mut destination = memory.read(r.address(0x220), dst.size)?.to_vec();
    for y in 0..h {
        for x in 0..w {
            let from = src.offset(sx + x, sy + y);
            let to = dst.offset(dx + x, dy + y);
            destination[to..to + 4].copy_from_slice(&source[from..from + 4]);
        }
    }
    memory.write(r.address(0x220), &destination)
}
