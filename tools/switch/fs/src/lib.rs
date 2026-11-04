//! Soju's own reader for Nintendo Switch container formats.
//!
//! Written from public format documentation (switchbrew wiki). No code from any
//! existing emulator or dumping tool is used. Keys are never bundled: callers pass
//! the player's own `prod.keys` / `title.keys`, and key material is never printed.

pub mod bktr;
pub mod cnmt;
pub mod crypto;
pub mod error;
pub mod hfs0;
pub mod keys;
pub mod lz4;
pub mod nacp;
pub mod nca;
pub mod npdm;
pub mod nso;
pub mod pfs0;
pub mod romfs;
pub mod ticket;
pub mod title;
pub mod xci;

pub use error::{Error, Result};

pub(crate) fn u16le(b: &[u8], at: usize) -> Result<u16> {
    b.get(at..at + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).ok_or(Error::Truncated)
}

pub(crate) fn u32le(b: &[u8], at: usize) -> Result<u32> {
    b.get(at..at + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).ok_or(Error::Truncated)
}

pub(crate) fn u64le(b: &[u8], at: usize) -> Result<u64> {
    b.get(at..at + 8).map(|s| u64::from_le_bytes(s.try_into().unwrap())).ok_or(Error::Truncated)
}

/// A NUL-terminated name inside a string table.
pub(crate) fn cstr(table: &[u8], at: usize) -> Result<String> {
    let rest = table.get(at..).ok_or(Error::Truncated)?;
    let end = rest.iter().position(|&c| c == 0).ok_or(Error::Format("unterminated name"))?;
    let name = std::str::from_utf8(&rest[..end]).map_err(|_| Error::Format("name is not UTF-8"))?;
    if name.is_empty() || name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return Err(Error::Format("unsafe file name"));
    }
    Ok(name.to_string())
}
