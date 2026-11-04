//! Clean-room GM20B graphics shader translation. See SOURCES.md for provenance.
pub mod decoder;
pub mod header;

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

pub mod ir;
pub mod msl;

/// Translate a complete 80-byte graphics SPH followed by scheduling/code bundles.
pub fn translate(binary: &[u8], options: &msl::Options) -> Result<msl::MetalShader> {
    let header = header::Header::parse(binary)?;
    let decoded = decoder::decode(&binary[header::HEADER_SIZE..])?;
    let program = ir::Program::build(&decoded)?;
    msl::generate(&header, &program, options)
}

pub mod cache;

pub mod compute;

pub mod reference;

pub mod texture;
