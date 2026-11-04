use super::*;

#[derive(Clone, Copy)]
pub(super) struct GraphicBuffer {
    pub id: u32,
    pub allocation: NvBufferView,
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub plane: u32,
    pub block: u32,
    pub format: u32,
}
impl GraphicBuffer {
    pub fn size(&self) -> Result<usize, Error> {
        if !(1..=4096).contains(&self.width)
            || !(1..=2160).contains(&self.height)
            || self.pitch < self.width * if self.format == 4 { 2 } else { 4 }
            || self.pitch > 65536
            || !self.pitch.is_multiple_of(64)
            || self.block > 5
            || !matches!(self.format, 1 | 2 | 4 | 5)
        {
            return Err(Error::Message(format!(
                "display geometry or format: {}x{} pitch={} block={} format={:#x}",
                self.width, self.height, self.pitch, self.block, self.format
            )));
        }
        let rows = 8u64 << self.block;
        let size = u64::from(self.pitch) * u64::from(self.height).div_ceil(rows) * rows;
        if size > MAX_BUFFER as u64
            || u64::from(self.plane) > self.allocation.size
            || size > self.allocation.size - u64::from(self.plane)
        {
            return Err(Error::Malformed("display outside allocation"));
        }
        usize::try_from(size).map_err(|_| Error::Malformed("frame size"))
    }
    pub fn fresh(&self, ctx: &Context<'_>) -> Result<(), Error> {
        if ctx.client != ClientId(1) {
            return Err(Error::Malformed("display client"));
        }
        let current = ctx
            .nv
            .ok_or(Error::Malformed("missing NV allocator"))?
            .resolve(ClientId(1), self.id)?;
        let saved = self.allocation;
        if current.allocation != saved.allocation
            || current.generation != saved.generation
            || current.guest_address != saved.guest_address
            || current.size != saved.size
            || current.kind != saved.kind
            || current.alignment != saved.alignment
        {
            return Err(Error::Malformed("stale display allocation"));
        }
        Ok(())
    }
    pub fn pixels(&self, ctx: &Context<'_>) -> Result<Frame, Error> {
        self.fresh(ctx)?;
        let size = self.size()?;
        let address = self
            .allocation
            .guest_address
            .checked_add(u64::from(self.plane))
            .ok_or(Error::Malformed("frame address"))?;
        let mut source = vec![0; size];
        ctx.memory.read(address, &mut source)?;
        let width = usize::try_from(self.width).map_err(|_| Error::Malformed("width"))?;
        let height = usize::try_from(self.height).map_err(|_| Error::Malformed("height"))?;
        let pitch = usize::try_from(self.pitch).map_err(|_| Error::Malformed("pitch"))?;
        let bh = 1usize << self.block;
        let rows = 8 * bh;
        let bytes_per_pixel = if self.format == 4 { 2 } else { 4 };
        let row_bytes = width * bytes_per_pixel;
        let mut pixels = vec![0; row_bytes * height];
        for y in 0..height {
            let base = (y / rows) * pitch * rows
                + ((y / 8) % bh) * 512
                + ((y % 8) / 2) * 64
                + (y % 2) * 16;
            for x in (0..row_bytes).step_by(16) {
                let offset =
                    base + (x / 64) * 512 * bh + ((x % 64) / 32) * 256 + ((x % 32) / 16) * 32;
                let len = (row_bytes - x).min(16);
                let chunk = source
                    .get(offset..offset + len)
                    .ok_or(Error::Malformed("block-linear address"))?;
                pixels[y * row_bytes + x..y * row_bytes + x + len].copy_from_slice(chunk);
            }
        }
        if self.format == 4 {
            // libnx framebuffer.h RGB565: little-endian B5/G6/R5.
            let mut rgba = Vec::with_capacity(width * height * 4);
            for pixel in pixels.chunks_exact(2) {
                let packed = u16::from_le_bytes([pixel[0], pixel[1]]);
                let r = u8::try_from((packed >> 11) & 31).map_err(|_| Error::Malformed("red"))?;
                let g = u8::try_from((packed >> 5) & 63).map_err(|_| Error::Malformed("green"))?;
                let b = u8::try_from(packed & 31).map_err(|_| Error::Malformed("blue"))?;
                rgba.extend_from_slice(&[
                    (r << 3) | (r >> 2),
                    (g << 2) | (g >> 4),
                    (b << 3) | (b >> 2),
                    255,
                ]);
            }
            pixels = rgba;
        }
        if self.format == 2 {
            for pixel in pixels.chunks_exact_mut(4) {
                pixel[3] = 255;
            }
        }
        if self.format == 5 {
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }
        Ok(Frame {
            width: self.width,
            height: self.height,
            pixels,
        })
    }
}
