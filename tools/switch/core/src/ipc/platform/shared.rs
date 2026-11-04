use super::*;
impl Platform {
    pub(super) fn hid_shared(
        &mut self,
        _: &Request,
        ctx: &mut Context<'_>,
    ) -> Result<Reply, Error> {
        if self.hid.is_none() {
            let mut bytes = vec![0; 0x40000];
            bytes[0x9a00] = 1;
            bytes[0x9a30] = 17;
            bytes[0x9a40] = 1;
            bytes[0x9a48..0x9a78].copy_from_slice(&hid_sample(self.sample, self.buttons));
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
                .write_shared(
                    memory,
                    handle,
                    0x9a48,
                    &hid_sample(self.sample, self.buttons),
                )
                .map_err(Error::Kernel)?;
        }
        if let Some(handle) = self.time {
            kernel
                .write_shared(memory, handle, 0, &time_image(self.clock.seconds(now_ns)?))
                .map_err(Error::Kernel)?;
        }
        Ok(())
    }
}
fn hid_sample(sample: u64, buttons: u64) -> [u8; 48] {
    let mut bytes = [0; 48];
    bytes[..8].copy_from_slice(&sample.to_le_bytes());
    bytes[8..16].copy_from_slice(&sample.to_le_bytes());
    bytes[16..24].copy_from_slice(&buttons.to_le_bytes());
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
