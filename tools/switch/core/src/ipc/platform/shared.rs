use super::*;
impl Platform {
    pub(super) fn hid_shared(
        &mut self,
        _: &Request,
        ctx: &mut Context<'_>,
    ) -> Result<Reply, Error> {
        if self.hid.is_none() {
            let mut bytes = vec![0; 0x40000];
            bytes[0x3c208] = 1; // The virtual console motion source is stationary.
            bytes[0x9a00] = 1;
            bytes[0x9a30] = 17;
            bytes[0x9a40] = 1;
            bytes[0x9a48..0x9a78].copy_from_slice(&hid_sample(self.sample, self.input));
            self.hid = Some(
                ctx.kernel
                    .register_shared(ctx.memory, &bytes, 1)
                    .map_err(Error::Kernel)?,
            );
        }
        Ok(Reply::copy(
            self.hid.ok_or(Error::Malformed("HID shared memory"))?,
        ))
    }
    pub(super) fn time_shared(
        &mut self,
        _: &Request,
        ctx: &mut Context<'_>,
    ) -> Result<Reply, Error> {
        if self.time.is_none() {
            let bytes = time_image(self.clock.seconds(ctx.now_ns)?);
            self.time = Some(
                ctx.kernel
                    .register_shared(ctx.memory, &bytes, 1)
                    .map_err(Error::Kernel)?,
            );
        }
        Ok(Reply::copy(
            self.time.ok_or(Error::Malformed("time shared memory"))?,
        ))
    }
    pub(super) fn update_shared(
        &mut self,
        kernel: &mut Kernel,
        memory: &mut dyn GuestMemory,
        now_ns: u64,
    ) -> Result<(), Error> {
        if let Some(handle) = self.hid {
            self.sample = self.sample.wrapping_add(1);
            kernel
                .write_shared(memory, handle, 0x9a48, &hid_sample(self.sample, self.input))
                .map_err(Error::Kernel)?;
            for &sensor in &self.six_axis {
                let offset = sensor_offset(sensor)?;
                let mut bytes = [0; 0x88];
                bytes[8..16].copy_from_slice(&17u64.to_le_bytes());
                bytes[24..32].copy_from_slice(&1u64.to_le_bytes());
                bytes[32..40].copy_from_slice(&self.sample.to_le_bytes());
                bytes[40..48]
                    .copy_from_slice(&now_ns.saturating_sub(self.sensor_last_ns).to_le_bytes());
                bytes[48..56].copy_from_slice(&self.sample.to_le_bytes());
                // Stationary virtual sensor: +Z gravity, zero rotation, identity orientation.
                for at in [64, 92, 108, 124] {
                    bytes[at..at + 4].copy_from_slice(&1f32.to_le_bytes());
                }
                bytes[128] = 1;
                kernel
                    .write_shared(memory, handle, offset, &bytes)
                    .map_err(Error::Kernel)?;
            }
        }
        self.sensor_last_ns = now_ns;
        if let Some(handle) = self.time {
            kernel
                .write_shared(memory, handle, 0, &time_image(self.clock.seconds(now_ns)?))
                .map_err(Error::Kernel)?;
        }
        Ok(())
    }
}
fn hid_sample(sample: u64, input: soju_switch_input::NpadState) -> [u8; 48] {
    let mut bytes = [0; 48];
    bytes[..8].copy_from_slice(&sample.to_le_bytes());
    bytes[8..16].copy_from_slice(&sample.to_le_bytes());
    bytes[16..24].copy_from_slice(&input.buttons.to_le_bytes());
    for (i, value) in [input.left.x, input.left.y, input.right.x, input.right.y]
        .iter()
        .enumerate()
    {
        bytes[24 + i * 4..28 + i * 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes[40] = 1;
    bytes
}
fn time_image(seconds: i64) -> Vec<u8> {
    let mut bytes = vec![0; 4096];
    for offset in [0x40, 0x60, 0x88, 0xa8] {
        bytes[offset..offset + 8].copy_from_slice(&seconds.to_le_bytes());
    }
    bytes
}

pub(super) fn sensor_offset(handle: u32) -> Result<usize, Error> {
    let player = match (handle >> 8) & 255 {
        0..=7 => (handle >> 8) & 255,
        0x20 => 8,
        0x10 => 9,
        _ => return Err(Error::Malformed("six-axis player")),
    };
    let device = (handle >> 16) & 255;
    if handle >> 24 != 0 {
        return Err(Error::Malformed("six-axis handle"));
    }
    let slot = match handle & 255 {
        3 if device == 2 => 0,
        4 if device == 2 => 1,
        5 if device <= 1 => 2 + device,
        6 if device == 0 => 4,
        7 if device == 1 => 5,
        _ => return Err(Error::Malformed("six-axis style")),
    };
    Ok(0x9a00 + player as usize * 0x5000 + 0x1758 + slot as usize * 0x708)
}
