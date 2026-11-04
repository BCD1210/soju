//! HFS0 (SHA-256 hashed partition), used inside XCI game card images.
//!
//! Layout: "HFS0", count u32, string table size u32, reserved u32, then 0x40-byte entries
//! (offset u64, size u64, name offset u32, hashed size u32, reserved u64, SHA-256 [0x20]).

use crate::pfs0::{Entry, Storage, MAX_FILES, MAX_STRINGS};
use crate::{cstr, u32le, u64le, Error, Result};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub struct HashedEntry {
    pub entry: Entry,
    pub hashed_size: u32,
    pub sha256: [u8; 32],
}

pub fn parse(s: &dyn Storage) -> Result<Vec<HashedEntry>> {
    let head = s.read_vec(0, 0x10)?;
    if &head[..4] != b"HFS0" {
        return Err(Error::BadMagic("HFS0"));
    }
    let count = u32le(&head, 4)?;
    let strings = u32le(&head, 8)?;
    if count > MAX_FILES || strings > MAX_STRINGS {
        return Err(Error::Format("HFS0 header is too large"));
    }
    let table_len = 0x10 + count as usize * 0x40 + strings as usize;
    let table = s.read_vec(0, table_len)?;
    let names = &table[0x10 + count as usize * 0x40..];
    let mut out = Vec::new();
    for i in 0..count as usize {
        let e = 0x10 + i * 0x40;
        let offset = (table_len as u64).checked_add(u64le(&table, e)?).ok_or(Error::Truncated)?;
        let size = u64le(&table, e + 8)?;
        if offset.checked_add(size).ok_or(Error::Truncated)? > s.len() {
            return Err(Error::Truncated);
        }
        out.push(HashedEntry {
            entry: Entry { name: cstr(names, u32le(&table, e + 0x10)? as usize)?, offset, size },
            hashed_size: u32le(&table, e + 0x14)?,
            sha256: table[e + 0x20..e + 0x40].try_into().unwrap(),
        });
    }
    Ok(out)
}

/// Check the hash of the first `hashed_size` bytes of an entry.
pub fn verify(s: &dyn Storage, e: &HashedEntry) -> Result<bool> {
    let n = (e.hashed_size as u64).min(e.entry.size) as usize;
    Ok(Sha256::digest(s.read_vec(e.entry.offset, n)?).as_slice() == e.sha256)
}

pub fn build(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut strings = Vec::new();
    let mut offsets = Vec::new();
    for (name, _) in files {
        offsets.push(strings.len() as u32);
        strings.extend_from_slice(name.as_bytes());
        strings.push(0);
    }
    while !(0x10 + files.len() * 0x40 + strings.len()).is_multiple_of(0x200) {
        strings.push(0);
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"HFS0");
    out.extend_from_slice(&(files.len() as u32).to_le_bytes());
    out.extend_from_slice(&(strings.len() as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    let mut off = 0u64;
    for (i, (_, data)) in files.iter().enumerate() {
        let hashed = data.len().min(0x200);
        out.extend_from_slice(&off.to_le_bytes());
        out.extend_from_slice(&(data.len() as u64).to_le_bytes());
        out.extend_from_slice(&offsets[i].to_le_bytes());
        out.extend_from_slice(&(hashed as u32).to_le_bytes());
        out.extend_from_slice(&0u64.to_le_bytes());
        out.extend_from_slice(&Sha256::digest(&data[..hashed]));
        off += data.len() as u64;
    }
    out.extend_from_slice(&strings);
    for (_, data) in files {
        out.extend_from_slice(data);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_hash() {
        let img = build(&[("secure", b"partition data"), ("normal", b"")]);
        let e = parse(&img).unwrap();
        assert_eq!(e[0].entry.name, "secure");
        assert!(verify(&img, &e[0]).unwrap());
        let mut bad = img.clone();
        let at = e[0].entry.offset as usize;
        bad[at] ^= 1;
        assert!(!verify(&bad, &e[0]).unwrap());
    }
}
