//! Maxwell QMD 0.6/1.7 launch fields from NVIDIA's public clb1c0qmd.h.
use crate::{decoder, header::Header, ir::Program, msl, Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub local_size: [u32; 3],
    pub shared_bytes: u32,
    pub local_positive: u32,
    pub local_negative: u32,
    pub raw: [u32; 64],
}
impl Launch {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 256 {
            return Err(Error("QMD must be exactly 256 bytes".into()));
        }
        let mut raw = [0; 64];
        for (word, chunk) in raw.iter_mut().zip(bytes.chunks_exact(4)) {
            *word = u32::from_le_bytes(chunk.try_into().map_err(|_| Error("QMD word".into()))?);
        }
        if !matches!(raw[18] & 255, 0x06 | 0x17) {
            return Err(Error("only QMD 0.6 and 1.7 are supported".into()));
        }
        let launch = Self {
            local_size: [raw[18] >> 16, raw[19] & 65535, raw[19] >> 16],
            shared_bytes: raw[17] & 0x3ffff,
            local_positive: raw[45] & 0xffffff,
            local_negative: raw[46] & 0xffffff,
            raw,
        };
        launch.validate()?;
        Ok(launch)
    }
    pub(crate) fn validate(&self) -> Result<()> {
        let count = self
            .local_size
            .iter()
            .try_fold(1u32, |a, b| a.checked_mul(*b));
        if !matches!(count, Some(1..=1024)) {
            return Err(Error(
                "compute local size must contain 1..1024 threads".into(),
            ));
        }
        if self.shared_bytes > 48 * 1024 || !self.shared_bytes.is_multiple_of(4) {
            return Err(Error(
                "shared allocation must be word-aligned and at most 48 KiB".into(),
            ));
        }
        if self
            .local_positive
            .checked_add(self.local_negative)
            .is_none_or(|n| n > 65536)
        {
            return Err(Error("local allocation exceeds 64 KiB".into()));
        }
        Ok(())
    }
    pub(crate) fn header(&self) -> Result<Header> {
        let mut bytes = [0; 80];
        bytes[..4].copy_from_slice(&1025u32.to_le_bytes());
        let mut h = Header::parse(&bytes)?;
        h.local_positive = self.local_positive;
        h.local_negative = self.local_negative;
        Ok(h)
    }
}

/// Code begins at its scheduling word; a compute program has no graphics SPH.
pub fn translate(code: &[u8], launch: &Launch, options: &msl::Options) -> Result<msl::MetalShader> {
    launch.validate()?;
    let program = Program::build(&decoder::decode(code)?)?;
    msl::generate_compute(launch, &program, options)
}
