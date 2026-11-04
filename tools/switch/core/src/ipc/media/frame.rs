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
            || self.pitch < self.width * 4
            || self.pitch > 65536
            || !self.pitch.is_multiple_of(64)
            || self.block > 5
            || !matches!(self.format, 1 | 5)
        {
            return Err(Error::Malformed("display geometry or format"));
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
        let mut pixels = vec![0; width * height * 4];
        for y in 0..height {
            let base = (y / rows) * pitch * rows
                + ((y / 8) % bh) * 512
                + ((y % 8) / 2) * 64
                + (y % 2) * 16;
            for x in (0..width * 4).step_by(16) {
                let offset =
                    base + (x / 64) * 512 * bh + ((x % 64) / 32) * 256 + ((x % 32) / 16) * 32;
                let len = (width * 4 - x).min(16);
                let chunk = source
                    .get(offset..offset + len)
                    .ok_or(Error::Malformed("block-linear address"))?;
                pixels[y * width * 4 + x..y * width * 4 + x + len].copy_from_slice(chunk);
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
